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

/// How to start the root helper.
#[derive(Debug, Clone, Default)]
pub struct Launch {
    /// The `bindle` binary to run as `bindle helper`; by default this program.
    pub exe: Option<PathBuf>,
    /// A desktop window, not a terminal: always ask through pkexec (polkit's
    /// password dialog) and report pkexec's own errors.
    pub graphical: bool,
}

/// pkexec found no polkit agent to ask for the password (some window
/// managers before Bindle installs one).
pub const NO_AGENT: &str = "no polkit authentication agent is running";

impl HelperClient {
    pub fn start(sandbox: Option<&Path>, log: &str) -> Result<Self> {
        Self::launch(sandbox, log, &Launch::default())
    }

    pub fn launch(sandbox: Option<&Path>, log: &str, how: &Launch) -> Result<Self> {
        let (exe, private_copy) = match &how.exe {
            Some(exe) => (exe.clone(), None),
            None => helper_binary()?,
        };
        let mut cmd = match sandbox {
            Some(root) => {
                let mut c = Command::new(&exe);
                c.args(["helper", "--fake-commands", "--root"]).arg(root);
                c
            }
            None => {
                let mut c = if how.graphical {
                    if !on_path("pkexec") {
                        bail!("pkexec is not installed, so Bindle can't ask for your password here");
                    }
                    Command::new("pkexec")
                } else if std::io::stdin().is_terminal() && on_path("sudo") {
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
        let stderr = if how.graphical { Stdio::piped() } else { Stdio::inherit() };
        cmd.args(["--log", log]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(stderr);
        let mut child = cmd.spawn().context("start the Bindle helper")?;
        // Keep what pkexec says (no agent, dismissed), drained so it can't block.
        let errors = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let mut reader = None;
        if let Some(err) = child.stderr.take() {
            let errors = errors.clone();
            reader = Some(std::thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    let mut e = errors.lock().unwrap_or_else(|p| p.into_inner());
                    if e.len() < 4096 {
                        e.push_str(&line);
                        e.push('\n');
                    }
                }
            }));
        }
        let stdin = child.stdin.take().context("helper stdin")?;
        let stdout = BufReader::new(child.stdout.take().context("helper stdout")?);
        let mut client = Self { child, stdin, stdout, private_copy };
        let greeting = client.next_event().map_err(|e| {
            // The pipe closed: pkexec refused or found no agent. Its message
            // arrives on stderr just before it exits.
            let _ = client.child.wait();
            if let Some(r) = reader.take() {
                let _ = r.join(); // stderr closed with the process: read it all
            }
            let said = errors.lock().map(|e| e.trim().to_string()).unwrap_or_default();
            if said.contains("authentication agent") {
                anyhow::anyhow!(NO_AGENT)
            } else if said.is_empty() {
                e
            } else {
                anyhow::anyhow!("{said}")
            }
        })?;
        match greeting {
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
