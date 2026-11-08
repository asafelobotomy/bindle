//! Small system primitives shared by the orchestrator and the helper.

use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, fchown};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

/// Map an absolute system path into `root` ("/" in production, a temp dir in
/// sandbox mode). Rejects relative paths and `..`.
pub fn host(root: &Path, path: &str) -> PathBuf {
    root.join(path.trim_start_matches('/'))
}

pub fn check_system_path(path: &str) -> Result<()> {
    let p = Path::new(path);
    if !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
        bail!("refusing unsafe path `{path}`");
    }
    Ok(())
}

/// Package or unit name safe to pass as an argument (no leading `-`).
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "@._+-:\\".contains(c))
}

pub fn is_root() -> bool {
    fs::metadata("/proc/self").is_ok_and(|m| m.uid() == 0)
}

/// Create `dir` and any missing parents as 0755, whatever the umask
/// (sudo passes the user's umask through; 077 would hide snapshots and logs).
pub fn create_dirs(dir: &Path) -> Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        create_dirs(parent)?;
    }
    match fs::create_dir(dir) {
        Ok(()) => fs::set_permissions(dir, fs::Permissions::from_mode(0o755))?,
        Err(e) if e.kind() == ErrorKind::AlreadyExists && dir.is_dir() => {}
        Err(e) => return Err(e).with_context(|| format!("create {}", dir.display())),
    }
    Ok(())
}

/// Write via a fresh temp file + rename so a crash never leaves a half-written
/// file. The temp file is created exclusively (never through a planted
/// symlink) with its final mode before any data lands; an existing target
/// keeps its owner, and a symlinked target is refused rather than replaced.
pub fn write_atomic(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    write_atomic_owned(path, data, mode, None)
}

/// `write_atomic`, giving the file to `owner` (uid, gid) when set.
pub fn write_atomic_owned(path: &Path, data: &[u8], mode: u32, owner: Option<(u32, u32)>) -> Result<()> {
    let dir = path.parent().context("path has no parent")?;
    create_dirs(dir)?;
    let owner = match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => bail!("refusing to replace symlink {}", path.display()),
        Ok(m) => owner.or(Some((m.uid(), m.gid()))),
        Err(_) => owner,
    };
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.bindle-{}-{nonce}", std::process::id()));
    let result = (|| -> Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .with_context(|| format!("create {}", tmp.display()))?;
        if let Some((uid, gid)) = owner {
            fchown(&f, Some(uid), Some(gid)).with_context(|| format!("chown {}", tmp.display()))?;
        }
        f.set_permissions(fs::Permissions::from_mode(mode))?;
        f.write_all(data)?;
        f.sync_all()?;
        fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn civil(secs: u64) -> (i64, u32, u32, u64) {
    // Howard Hinnant's days-to-civil.
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d, secs % 86_400)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// `20261008-153012` (UTC), sortable; used for snapshot ids and log names.
pub fn now_id() -> String {
    let (y, m, d, s) = civil(now_secs());
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", s / 3600, s / 60 % 60, s % 60)
}

pub fn now_rfc3339() -> String {
    let (y, m, d, s) = civil(now_secs());
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, s / 60 % 60, s % 60)
}

pub fn shell_words(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if !a.is_empty() && a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c)) {
                a.clone()
            } else {
                format!("'{}'", a.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Hold an exclusive lock on `path` (created 0600) for as long as the
/// returned file is open; fails at once if another process holds it.
pub fn lock_exclusive(path: &Path) -> Result<fs::File> {
    if let Some(dir) = path.parent() {
        create_dirs(dir)?;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    // SAFETY: flock on a file descriptor we own; no memory is passed.
    if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&file), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        bail!("another Bindle apply or undo is running (lock {})", path.display());
    }
    Ok(file)
}

/// Ignore Ctrl-C (`true`) or restore the default (`false`). Used around root
/// steps: interrupting pacman or the helper mid-step can leave the system
/// half-changed, and pacman handles its own interrupts safely.
pub fn ignore_interrupts(ignore: bool) {
    let handler = if ignore { libc::SIG_IGN } else { libc::SIG_DFL };
    // SAFETY: installs a disposition constant, not a Rust handler.
    unsafe {
        libc::signal(libc::SIGINT, handler);
    }
}

/// Run a command, passing each stdout/stderr line to `on_line` as it arrives.
pub fn run_streaming(argv: &[String], on_line: &mut dyn FnMut(&str)) -> Result<()> {
    let (prog, args) = argv.split_first().context("empty command")?;
    let mut child = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start `{prog}`"))?;
    let (tx, rx) = mpsc::channel::<String>();
    let readers: Vec<_> = [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|stream| {
        let tx = tx.clone();
        // Read raw bytes: a non-UTF-8 line must not stop the drain, or the
        // child (pacman mid-transaction) dies of SIGPIPE.
        thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut buf = Vec::new();
            while matches!(reader.read_until(b'\n', &mut buf), Ok(n) if n > 0) {
                let line = String::from_utf8_lossy(&buf);
                let _ = tx.send(line.trim_end_matches(['\n', '\r']).to_string());
                buf.clear();
            }
        })
    })
    .collect();
    drop(tx);
    for line in rx {
        on_line(&line);
    }
    readers.into_iter().for_each(|r| drop(r.join()));
    let status = child.wait()?;
    if !status.success() {
        bail!("`{}` failed ({status})", shell_words(argv));
    }
    Ok(())
}

/// A fresh scratch directory for tests (here and in bindle-cli).
#[doc(hidden)]
pub fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bindle-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_paths() {
        assert_eq!(civil(0), (1970, 1, 1, 0));
        assert_eq!(civil(1_791_461_412).0, 2026);
        assert!(check_system_path("/etc/x").is_ok());
        assert!(check_system_path("/etc/../root/x").is_err());
        assert!(check_system_path("etc/x").is_err());
        assert_eq!(host(Path::new("/tmp/r"), "/etc/a"), PathBuf::from("/tmp/r/etc/a"));
        assert_eq!(shell_words(&["echo".into(), "a b".into()]), "echo 'a b'");
        assert!(valid_name("lib32-mesa") && valid_name("getty@tty1.service") && !valid_name("--root=/"));
    }

    #[test]
    fn atomic_write_keeps_mode_and_refuses_symlinks() {
        let dir = test_dir("atomic");
        let target = dir.join("a/b/file.conf");
        write_atomic(&target, b"one", 0o640).unwrap();
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o640);
        assert_eq!(fs::metadata(dir.join("a/b")).unwrap().permissions().mode() & 0o777, 0o755);
        write_atomic(&target, b"two", 0o644).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "two");
        std::os::unix::fs::symlink(&target, dir.join("link")).unwrap();
        assert!(write_atomic(&dir.join("link"), b"x", 0o644).is_err());
        assert_eq!(fs::read_dir(dir.join("a/b")).unwrap().count(), 1, "no temp files left behind");
    }

    #[test]
    fn streams_output_and_reports_failure() {
        let mut lines = Vec::new();
        run_streaming(&["sh".into(), "-c".into(), "echo out; echo err >&2".into()], &mut |l| lines.push(l.to_string()))
            .unwrap();
        lines.sort();
        assert_eq!(lines, ["err", "out"]);
        assert!(run_streaming(&["false".into()], &mut |_| {}).is_err());
    }

    #[test]
    fn lock_is_exclusive_and_refuses_symlinks() {
        let dir = test_dir("lock");
        let held = lock_exclusive(&dir.join("lock")).unwrap();
        assert!(lock_exclusive(&dir.join("lock")).is_err(), "second holder must be refused");
        drop(held);
        assert!(lock_exclusive(&dir.join("lock")).is_ok());
        std::os::unix::fs::symlink(dir.join("lock"), dir.join("link")).unwrap();
        assert!(lock_exclusive(&dir.join("link")).is_err());
    }

    #[test]
    fn non_utf8_output_keeps_draining() {
        // A bad byte followed by more output than a pipe buffer holds: the
        // child must still finish successfully and every line must arrive.
        let script = "printf 'bad \\377\\n'; i=0; while [ $i -lt 20000 ]; do echo line$i; i=$((i+1)); done";
        let mut count = 0;
        let mut first = String::new();
        run_streaming(&["sh".into(), "-c".into(), script.into()], &mut |l| {
            if count == 0 {
                first = l.to_string();
            }
            count += 1;
        })
        .unwrap();
        assert_eq!(count, 20001);
        assert_eq!(first, "bad \u{FFFD}");
    }
}
