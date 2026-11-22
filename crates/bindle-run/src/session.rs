//! One run's way of doing things as root and as the user: Bindle's root
//! helper, started when the first root command is due (one password prompt
//! for the run), a refusal remembered so later steps fail at once instead of
//! asking again, and dry runs that only print. Shared by bindleup and
//! the recovery tools.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use bindle_core::Step;

use crate::bridge::Bridge;
use crate::client::{HelperClient, Launch};
use crate::protocol::Request;
use crate::sys;

pub struct Session<'a> {
    /// Print every command, run none, start no root helper.
    pub dry_run: bool,
    /// Root commands go to a helper that only records them, against this
    /// directory; user commands are printed.
    pub sandbox: Option<PathBuf>,
    launch: &'a Launch,
    log: String,
    helper: Option<HelperClient>,
    /// The helper couldn't start (password dismissed, no polkit agent).
    refused: Option<String>,
}

impl<'a> Session<'a> {
    /// `log`: the helper's log file (under /var/log/bindle/).
    pub fn new(log: String, dry_run: bool, sandbox: Option<PathBuf>, launch: &'a Launch) -> Self {
        Self { dry_run, sandbox, launch, log, helper: None, refused: None }
    }

    /// As if the helper had already been refused: every root request fails
    /// with `why` without asking (for tests of what follows a refusal).
    pub fn refused(mut self, why: impl Into<String>) -> Self {
        self.refused = Some(why.into());
        self
    }

    /// Nothing really runs: a dry run or a sandbox.
    pub fn rehearsal(&self) -> bool {
        self.dry_run || self.sandbox.is_some()
    }

    /// The helper, if this run has started it.
    pub fn helper(&mut self) -> Option<&mut HelperClient> {
        self.helper.as_mut()
    }

    /// A request to the root helper, starting it the first time.
    pub fn root(&mut self, req: &Request, line: &mut dyn FnMut(&str)) -> Result<()> {
        if self.dry_run {
            let shown = match req {
                Request::Run { step: Step::RunAsRoot { argv, .. } } => sys::shell_words(argv),
                Request::Interactive { argv, .. } => format!("{} (on this terminal)", sys::shell_words(argv)),
                Request::RunSnapshotPre => {
                    "snapper -c root create -t pre -d bindleup (snap-pac waits until the run ends)".into()
                }
                Request::RunSnapshotPost => "snapper -c root create -t post -d bindleup (the run's pair)".into(),
                other => format!("{other:?}"),
            };
            line(&format!("would run as root: {shown}"));
            return Ok(());
        }
        if let Some(why) = &self.refused {
            bail!("{why}");
        }
        self.start(line)?;
        let helper = self.helper.as_mut().expect("started above");
        helper.request(req, line).map(drop)
    }

    /// Start the root helper now (the password is asked here, once), so the
    /// rest of the run needs nobody. Does nothing when it already runs, in a
    /// rehearsal, or after a refusal (the root steps then say why).
    pub fn start(&mut self, line: &mut dyn FnMut(&str)) -> Result<()> {
        if self.dry_run || self.helper.is_some() {
            return Ok(());
        }
        if let Some(why) = &self.refused {
            bail!("{why}");
        }
        line("Asking for administrator rights (once for this run).");
        match HelperClient::launch(self.sandbox.as_deref(), &self.log, self.launch) {
            Ok(h) => {
                self.helper = Some(h);
                Ok(())
            }
            Err(e) => {
                let why = format!("couldn't get administrator rights: {e:#}");
                self.refused = Some(why.clone());
                bail!(why);
            }
        }
    }

    /// An AUR helper (on the terminal, or line by line without one) whose
    /// `sudo pacman …` calls go to this run's root helper (see
    /// [`crate::bridge`]): no password while it builds and installs. Other
    /// commands and a rehearsal behave as [`Session::user`].
    pub fn user_elevated(&mut self, argv: &[String], tty: Option<&str>, line: &mut dyn FnMut(&str)) -> Result<()> {
        let known = argv.first().is_some_and(|p| matches!(p.as_str(), "paru" | "yay"));
        if !known || self.rehearsal() {
            return self.user(argv, tty.is_some(), line);
        }
        self.start(line)?;
        let exe = self.shim_exe()?;
        let bridge = Bridge::start(&exe)?;
        let shown = sys::shell_words(argv);
        if let Some(h) = self.helper.as_mut() {
            let _ = h.request(
                &Request::Log { line: format!("(as the user, its pacman calls as root) $ {shown}") },
                &mut |_| {},
            );
        }
        let mut child = Command::new(&argv[0]);
        child.args(["--sudo", &bridge.shim()]).args(&argv[1..]).envs(bridge.env());
        if let Some(flags) = parallel_make() {
            child.env("MAKEFLAGS", flags);
        }
        // Without a terminal (the window, uQ Settings) the helper's output comes line by line,
        // and its pacman calls run through the root helper without one (they carry --noconfirm).
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        if tty.is_none() {
            child.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        }
        let mut child = child.spawn().with_context(|| format!("start `{}`", argv[0]))?;
        let readers: Vec<_> = [
            child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
            child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        .map(|stream| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut buf = Vec::new();
                while matches!(reader.read_until(b'\n', &mut buf), Ok(n) if n > 0) {
                    let _ = tx.send(String::from_utf8_lossy(&buf).trim_end_matches(['\n', '\r']).to_string());
                    buf.clear();
                }
            })
        })
        .collect();
        drop(tx);
        let status = loop {
            bridge.poll(&mut |pacman| {
                // pacman (as root) ignores Ctrl-C mid-transaction; so does this side.
                sys::ignore_interrupts(true);
                let req = match tty {
                    Some(tty) => Request::Interactive { argv: pacman, tty: tty.to_string() },
                    None => Request::Run { step: Step::RunAsRoot { module: "bindleup".into(), argv: pacman } },
                };
                let done = match self.helper.as_mut() {
                    Some(h) => h.request(&req, &mut |l| line(l)),
                    None => Err(anyhow::anyhow!("the root helper isn't running")),
                };
                sys::ignore_interrupts(false);
                done.map(drop)
            });
            while let Ok(l) = rx.try_recv() {
                line(&l);
            }
            if let Some(s) = child.try_wait()? {
                break s;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        };
        for r in readers {
            let _ = r.join();
        }
        while let Ok(l) = rx.try_recv() {
            line(&l);
        }
        if !status.success() {
            bail!("`{shown}` failed ({status})");
        }
        Ok(())
    }

    /// The `bindle` program the AUR helper's `sudo` shim runs: the one the
    /// root helper runs, not this program. In the update window this program
    /// is `bindle-gui`, which opened a second window as paru's `sudo` and
    /// left the run waiting on it.
    fn shim_exe(&self) -> Result<PathBuf> {
        match &self.launch.exe {
            Some(exe) => Ok(exe.clone()),
            None => std::env::current_exe().context("find this program"),
        }
    }

    /// A command as the user: on the terminal when there is one (`tty`),
    /// otherwise line by line through `line` (the window).
    pub fn user(&mut self, argv: &[String], tty: bool, line: &mut dyn FnMut(&str)) -> Result<()> {
        let shown = sys::shell_words(argv);
        if self.rehearsal() {
            line(&format!("would run: {shown}"));
            return Ok(());
        }
        if let Some(h) = self.helper.as_mut() {
            let _ = h.request(&Request::Log { line: format!("(as the user) $ {shown}") }, &mut |_| {});
        }
        if !tty {
            return sys::run_streaming(argv, line);
        }
        let (prog, args) = argv.split_first().context("empty command")?;
        let status = Command::new(prog).args(args).status().with_context(|| format!("start `{prog}`"))?;
        if !status.success() {
            bail!("`{shown}` failed ({status})");
        }
        Ok(())
    }

    /// End the helper, if it started.
    pub fn quit(&mut self) {
        if let Some(h) = self.helper.take() {
            h.quit();
        }
    }
}

/// `-jN` for the AUR builds when neither the environment nor makepkg.conf
/// chose: makepkg builds on one core by default.
fn parallel_make() -> Option<String> {
    if std::env::var_os("MAKEFLAGS").is_some() {
        return None;
    }
    let conf = std::fs::read_to_string("/etc/makepkg.conf").unwrap_or_default();
    if conf.lines().any(|l| l.trim_start().starts_with("MAKEFLAGS=")) {
        return None;
    }
    let n = std::thread::available_parallelism().map_or(1, |n| n.get());
    (n > 1).then(|| format!("-j{n}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shim_runs_the_root_helpers_bindle_not_the_window() {
        // The update window launches with `bindle` as the helper; its own
        // program is `bindle-gui`, which must never be paru's `sudo`.
        let launch = Launch { exe: Some(PathBuf::from("/usr/bin/bindle")), graphical: true };
        let s = Session::new(String::new(), false, None, &launch);
        assert_eq!(s.shim_exe().unwrap(), PathBuf::from("/usr/bin/bindle"));
        let default = Launch::default();
        let s = Session::new(String::new(), false, None, &default);
        assert_eq!(s.shim_exe().unwrap(), std::env::current_exe().unwrap());
    }
}
