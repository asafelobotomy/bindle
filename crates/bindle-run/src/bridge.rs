//! Lets an AUR helper's `sudo pacman …` calls reach the run's one root
//! helper, so building and installing AUR packages asks for no further
//! password. The helper (paru, yay) is started with `--sudo <shim>`; the
//! shim (`bindle elev-shim`) connects to a socket that only this user can
//! reach, and the run forwards the pacman command to the root helper.
//!
//! What it allows is small on purpose: `pacman` only, never with options
//! that move its root, databases, hooks, config or logs, and `-U` only with
//! package files. The socket exists only while the helper runs, in a 0700
//! directory, and each request carries this run's random token.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub const SOCKET_ENV: &str = "BINDLE_ELEV_SOCK";
pub const TOKEN_ENV: &str = "BINDLE_ELEV_TOKEN";

#[derive(Serialize, Deserialize)]
struct Ask {
    token: String,
    argv: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Answer {
    error: Option<String>,
}

pub struct Bridge {
    dir: PathBuf,
    listener: UnixListener,
    token: String,
}

impl Bridge {
    /// `exe`: the `bindle` program the shim runs.
    pub fn start(exe: &Path) -> Result<Self> {
        let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).context("no XDG_RUNTIME_DIR")?;
        let token = random_token()?;
        let dir = base.join(format!("bindle-elev-{}-{}", std::process::id(), &token[..8]));
        let _ = fs::remove_dir_all(&dir);
        fs::DirBuilder::new().mode(0o700).create(&dir).with_context(|| format!("create {}", dir.display()))?;
        let listener = UnixListener::bind(dir.join("sock")).context("open the bridge socket")?;
        listener.set_nonblocking(true)?;
        let shim = format!("#!/bin/sh\nexec '{}' elev-shim \"$@\"\n", exe.display().to_string().replace('\'', "'\\''"));
        let path = dir.join("sudo");
        fs::write(&path, shim)?;
        fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
        Ok(Self { dir, listener, token })
    }

    /// The program to give the AUR helper as its `sudo`.
    pub fn shim(&self) -> String {
        self.dir.join("sudo").display().to_string()
    }

    /// The environment the helper (and so the shim) needs.
    pub fn env(&self) -> [(&'static str, String); 2] {
        [(SOCKET_ENV, self.dir.join("sock").display().to_string()), (TOKEN_ENV, self.token.clone())]
    }

    /// Serve one waiting request, if any: `run` gets the checked pacman
    /// command (as root, it must fail or succeed with a message).
    pub fn poll(&self, run: &mut dyn FnMut(Vec<String>) -> Result<()>) {
        let Ok((stream, _)) = self.listener.accept() else { return };
        let _ = stream.set_nonblocking(false);
        let answer = match self.read(&stream) {
            Ok(argv) => run(argv),
            Err(e) => Err(e),
        };
        let reply = Answer { error: answer.err().map(|e| format!("{e:#}")) };
        let mut stream = stream;
        let _ = writeln!(stream, "{}", serde_json::to_string(&reply).unwrap_or_default());
    }

    fn read(&self, stream: &UnixStream) -> Result<Vec<String>> {
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line)?;
        let ask: Ask = serde_json::from_str(&line).context("bad request")?;
        if ask.token != self.token {
            bail!("not this run's request");
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let cmd = allowed(&ask.argv, &home)?;
        package_files_trusted(&cmd)?;
        Ok(cmd)
    }
}

/// `pacman -U` runs a package's install scripts as root, so a file only counts when nobody
/// else could have written it: a plain file (not a link) owned by this user or by root, that
/// neither group nor others can write. `/tmp` and `/var/tmp` are shared, so the owner check
/// is what keeps another account's package out. Checked again by pacman's own signature
/// rules; this only stops the bridge from handing root to a file it can't vouch for.
fn package_files_trusted(cmd: &[String]) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    if !cmd.iter().skip(1).any(|a| a == "--upgrade" || (a.starts_with('-') && !a.starts_with("--") && a.contains('U')))
    {
        return Ok(());
    }
    // SAFETY: getuid has no preconditions and cannot fail.
    let me = unsafe { libc::getuid() };
    for a in cmd.iter().skip(1).filter(|a| !a.starts_with('-')) {
        let meta = fs::symlink_metadata(a).with_context(|| format!("can't look at `{a}`"))?;
        if !meta.is_file() || (meta.uid() != me && meta.uid() != 0) || meta.mode() & 0o022 != 0 {
            bail!("`{a}` must be a plain package file owned by you that others can't write");
        }
    }
    Ok(())
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn random_token() -> Result<String> {
    let mut bytes = [0u8; 24];
    std::io::Read::read_exact(&mut fs::File::open("/dev/urandom")?, &mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Options that would point pacman somewhere else than the system.
const MOVING: [&str; 9] =
    ["--root", "--dbpath", "--hookdir", "--config", "--cachedir", "--logfile", "--gpgdir", "--sysroot", "--arch"];

/// The pacman command to run as root, or why not: `argv` is what the AUR
/// helper called its sudo with (`pacman`, then pacman's arguments).
pub fn allowed(argv: &[String], home: &str) -> Result<Vec<String>> {
    let (prog, args) = argv.split_first().context("empty command")?;
    if Path::new(prog).file_name().and_then(|n| n.to_str()) != Some("pacman") {
        bail!("only pacman can be run this way, not `{prog}`");
    }
    let mut out = vec!["/usr/bin/pacman".to_string()];
    let mut op = None;
    for a in args {
        if let Some(long) = a.strip_prefix("--") {
            if MOVING.iter().any(|m| m.trim_start_matches("--") == long.split('=').next().unwrap_or("")) {
                bail!("pacman {a} isn't allowed here");
            }
            let long_op = ["sync", "upgrade", "remove", "database", "query"].into_iter().find(|o| *o == long);
            op = op.or(long_op.map(|o| o.chars().next().unwrap_or('?').to_ascii_uppercase()));
        } else if let Some(short) = a.strip_prefix('-').filter(|s| !s.is_empty()) {
            // -r and -b take a path and move pacman's root or database.
            if short.chars().any(|c| matches!(c, 'r' | 'b')) {
                bail!("pacman {a} isn't allowed here");
            }
            op = op.or(short.chars().find(|c| matches!(c, 'S' | 'U' | 'R' | 'D' | 'Q')));
        } else if op == Some('U') {
            let p = Path::new(a);
            let name = a.rsplit('/').next().unwrap_or("");
            let place = [home, "/var/cache", "/tmp", "/var/tmp"].iter().any(|d| !d.is_empty() && p.starts_with(d));
            if !p.is_absolute() || a.split('/').any(|c| c == "..") || !name.contains(".pkg.tar") || !place {
                bail!("pacman -U only takes package files (.pkg.tar.*) under your home, /var/cache or /tmp, not `{a}`");
            }
        }
        out.push(a.clone());
    }
    if op.is_none() {
        bail!("pacman without an operation");
    }
    Ok(out)
}

/// `bindle elev-shim <command…>`: what the AUR helper runs as its sudo.
/// Returns the exit status.
pub fn shim(args: &[String]) -> i32 {
    // The root command ignores Ctrl-C mid-transaction; so does this side.
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
    }
    match ask(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("bindleup: {e:#}");
            1
        }
    }
}

fn ask(args: &[String]) -> Result<()> {
    let (Some(sock), Some(token)) = (std::env::var_os(SOCKET_ENV), std::env::var(TOKEN_ENV).ok()) else {
        bail!("this is for bindleup's AUR step only");
    };
    // A path that exists relative to here is made absolute: the root side runs elsewhere.
    let argv = args
        .iter()
        .map(|a| match (a.starts_with('-'), Path::new(a).canonicalize()) {
            (false, Ok(p)) if Path::new(a).is_relative() => p.display().to_string(),
            _ => a.clone(),
        })
        .collect();
    let mut stream = UnixStream::connect(sock).context("bindleup isn't listening any more")?;
    writeln!(stream, "{}", serde_json::to_string(&Ask { token, argv })?)?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    match serde_json::from_str::<Answer>(&line).context("no answer from bindleup")?.error {
        None => Ok(()),
        Some(e) => bail!("{e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn pacman_installs_and_removals_pass() {
        let ok = |a: &[&str]| allowed(&v(a), "/home/me");
        assert_eq!(ok(&["pacman", "-S", "--asdeps", "--needed", "foo"]).unwrap()[0], "/usr/bin/pacman");
        assert!(ok(&["/usr/bin/pacman", "-Rns", "--noconfirm", "foo"]).is_ok());
        assert!(ok(&["pacman", "-U", "--noconfirm", "/home/me/.cache/paru/clone/x/x-1-1-any.pkg.tar.zst"]).is_ok());
        assert!(ok(&["pacman", "--upgrade", "/var/cache/pacman/pkg/x-1-1-any.pkg.tar.zst"]).is_ok());
        assert!(ok(&["pacman", "-D", "--asexplicit", "foo"]).is_ok());
    }

    #[test]
    fn everything_else_is_refused() {
        let no = |a: &[&str]| allowed(&v(a), "/home/me").unwrap_err().to_string();
        assert!(no(&["sh", "-c", "id"]).contains("only pacman"));
        assert!(no(&["pacman", "-S", "--hookdir=/tmp/h", "x"]).contains("isn't allowed"));
        assert!(no(&["pacman", "-S", "--root", "/tmp/r", "x"]).contains("isn't allowed"));
        assert!(no(&["pacman", "-Sr", "/tmp/r", "x"]).contains("isn't allowed"));
        assert!(no(&["pacman", "-U", "/etc/passwd"]).contains("package files"));
        assert!(no(&["pacman", "-U", "/home/me/../root/x.pkg.tar.zst"]).contains("package files"));
        assert!(no(&["pacman", "-U", "relative.pkg.tar.zst"]).contains("package files"));
        assert!(no(&["pacman", "-U", "/opt/x.pkg.tar.zst"]).contains("package files"));
        assert!(no(&["pacman", "foo"]).contains("operation"));
        assert!(no(&[]).contains("empty"));
    }

    #[test]
    fn a_package_file_must_be_a_plain_file_nobody_else_can_write() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = std::env::temp_dir().join(format!("bindle-pkg-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let pkg = dir.join("x-1-1-any.pkg.tar.zst");
        fs::write(&pkg, b"x").unwrap();
        fs::set_permissions(&pkg, fs::Permissions::from_mode(0o644)).unwrap();
        let cmd = |p: &Path| v(&["/usr/bin/pacman", "-U", "--noconfirm", &p.display().to_string()]);
        assert!(package_files_trusted(&cmd(&pkg)).is_ok());
        fs::set_permissions(&pkg, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(package_files_trusted(&cmd(&pkg)).is_err(), "world-writable");
        fs::set_permissions(&pkg, fs::Permissions::from_mode(0o664)).unwrap();
        assert!(package_files_trusted(&cmd(&pkg)).is_err(), "group-writable");
        let link = dir.join("link.pkg.tar.zst");
        symlink(&pkg, &link).unwrap();
        fs::set_permissions(&pkg, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(package_files_trusted(&cmd(&link)).is_err(), "a symlink");
        assert!(package_files_trusted(&cmd(&dir.join("missing.pkg.tar.zst"))).is_err());
        // Not an install from a file: nothing to check.
        assert!(package_files_trusted(&v(&["/usr/bin/pacman", "-S", "foo"])).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bridge_serves_only_its_own_token() {
        let dir = std::env::temp_dir().join(format!("bindle-bridge-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // SAFETY: tests in this crate don't read XDG_RUNTIME_DIR concurrently.
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &dir) };
        let b = Bridge::start(Path::new("/usr/bin/bindle")).unwrap();
        let sock = b.dir.join("sock");
        let send = |token: &str, argv: &[&str]| {
            let mut s = UnixStream::connect(&sock).unwrap();
            writeln!(s, "{}", serde_json::to_string(&Ask { token: token.into(), argv: v(argv) }).unwrap()).unwrap();
            let t = token.to_string();
            let h = std::thread::spawn(move || {
                let mut line = String::new();
                BufReader::new(s).read_line(&mut line).unwrap();
                (t, serde_json::from_str::<Answer>(&line).unwrap().error)
            });
            std::thread::sleep(Duration::from_millis(50));
            h
        };
        let mut ran = vec![];
        let bad = send("wrong", &["pacman", "-Syu"]);
        b.poll(&mut |a| {
            ran.push(a);
            Ok(())
        });
        assert!(bad.join().unwrap().1.unwrap().contains("not this run"));
        let good = send(&b.token.clone(), &["pacman", "-Syu"]);
        b.poll(&mut |a| {
            ran.push(a);
            Ok(())
        });
        assert!(good.join().unwrap().1.is_none());
        assert_eq!(ran, [v(&["/usr/bin/pacman", "-Syu"])]);
        drop(b);
        assert!(!sock.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_shim_gets_the_command_through_and_the_answer_back() {
        let dir = std::env::temp_dir().join(format!("bindle-shim-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // SAFETY: tests in this crate don't read these variables concurrently.
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &dir) };
        let b = Bridge::start(Path::new("/usr/bin/bindle")).unwrap();
        for (k, val) in b.env() {
            unsafe { std::env::set_var(k, val) };
        }
        let call = |args: Vec<String>| std::thread::spawn(move || ask(&args));
        for (args, fail) in [(v(&["pacman", "-S", "foo"]), false), (v(&["pacman", "-S", "bad"]), true)] {
            let h = call(args);
            let mut got = vec![];
            while !h.is_finished() {
                b.poll(&mut |a| {
                    got.push(a.clone());
                    if fail { Err(anyhow::anyhow!("pacman failed (exit status: 1)")) } else { Ok(()) }
                });
                std::thread::sleep(Duration::from_millis(10));
            }
            let r = h.join().unwrap();
            assert_eq!(r.is_err(), fail);
            if fail {
                assert!(r.unwrap_err().to_string().contains("pacman failed"));
            }
            assert_eq!(got.len(), 1);
        }
        drop(b);
        let _ = fs::remove_dir_all(&dir);
    }
}
