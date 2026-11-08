//! Starts the root helper and talks to it.

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::protocol::{Event, Request, VERSION};

pub struct HelperClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    private_copy: Option<PathBuf>,
}

fn on_path(prog: &str) -> bool {
    env::var_os("PATH").is_some_and(|p| env::split_paths(&p).any(|d| d.join(prog).is_file()))
}

/// Root can't read files inside a user's FUSE mount, so an AppImage's binary
/// is copied to a private, root-readable directory first.
fn helper_binary() -> Result<(PathBuf, Option<PathBuf>)> {
    let exe = env::current_exe().context("locate own executable")?;
    if env::var_os("APPIMAGE").is_none() {
        return Ok((exe, None));
    }
    // A directory only this user can create in: never a guessable path in /tmp.
    let base = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/bindle")))
        .context("neither XDG_RUNTIME_DIR nor HOME is set")?;
    fs::create_dir_all(&base)?;
    let dir = base.join(format!("bindle-helper-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::DirBuilder::new().mode(0o700).create(&dir).with_context(|| format!("create {}", dir.display()))?;
    let copy = dir.join("bindle");
    fs::copy(&exe, &copy).context("copy helper out of the AppImage")?;
    fs::set_permissions(&copy, fs::Permissions::from_mode(0o700))?;
    Ok((copy, Some(dir)))
}

impl HelperClient {
    pub fn start(sandbox: Option<&Path>, log: &str) -> Result<Self> {
        let (exe, private_copy) = helper_binary()?;
        let mut cmd = match sandbox {
            Some(root) => {
                let mut c = Command::new(&exe);
                c.args(["helper", "--fake-commands", "--root"]).arg(root);
                c
            }
            None => {
                let mut c = if std::io::stdin().is_terminal() && on_path("sudo") {
                    println!("Bindle needs administrator rights for system changes.");
                    // `sudo true`, not `sudo -v`: -v demands a password unless *every*
                    // sudoers entry is NOPASSWD, while `true` follows the same rules as
                    // the helper command. It still caches credentials for password users.
                    let ok = Command::new("sudo").arg("true").status().is_ok_and(|s| s.success());
                    if !ok {
                        bail!("could not get administrator rights via sudo");
                    }
                    Command::new("sudo")
                } else if on_path("pkexec")
                    && (env::var_os("DISPLAY").is_some() || env::var_os("WAYLAND_DISPLAY").is_some())
                {
                    Command::new("pkexec")
                } else {
                    bail!("no way to get administrator rights: run Bindle from a terminal with sudo installed");
                };
                c.arg(&exe).arg("helper");
                c
            }
        };
        cmd.args(["--log", log]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        let mut child = cmd.spawn().context("start the Bindle helper")?;
        let stdin = child.stdin.take().context("helper stdin")?;
        let stdout = BufReader::new(child.stdout.take().context("helper stdout")?);
        let mut client = Self { child, stdin, stdout, private_copy };
        match client.next_event()? {
            Event::Hello { version } if version == VERSION => Ok(client),
            Event::Hello { version } => bail!("helper version {version} does not match {VERSION}"),
            other => bail!("unexpected helper greeting: {other:?}"),
        }
    }

    fn next_event(&mut self) -> Result<Event> {
        let mut line = String::new();
        if self.stdout.read_line(&mut line)? == 0 {
            bail!("the helper exited unexpectedly (authentication cancelled?)");
        }
        serde_json::from_str(&line).with_context(|| format!("bad helper event: {line}"))
    }

    /// Send a request; stream output lines to `on_output` until it finishes.
    pub fn request(&mut self, req: &Request, on_output: &mut dyn FnMut(&str)) -> Result<Option<String>> {
        writeln!(self.stdin, "{}", serde_json::to_string(req)?)?;
        self.stdin.flush()?;
        loop {
            match self.next_event()? {
                Event::Output { line } => on_output(&line),
                Event::Done => return Ok(None),
                Event::Snapshot { id } => return Ok(Some(id)),
                Event::Failed { error } => bail!(error),
                Event::Hello { .. } => {}
            }
        }
    }

    pub fn quit(mut self) {
        let _ = writeln!(self.stdin, "{}", serde_json::to_string(&Request::Quit).unwrap_or_default());
        let _ = self.child.wait();
    }
}

impl Drop for HelperClient {
    fn drop(&mut self) {
        if let Some(dir) = self.private_copy.take() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}
