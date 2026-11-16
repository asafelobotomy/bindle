//! One run's way of doing things as root and as the user: Bindle's root
//! helper, started when the first root command is due (one password prompt
//! for the run), a refusal remembered so later steps fail at once instead of
//! asking again, and dry runs that only print. Shared by bindleup and
//! the recovery tools.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use bindle_core::Step;

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
                other => format!("{other:?}"),
            };
            line(&format!("would run as root: {shown}"));
            return Ok(());
        }
        if let Some(why) = &self.refused {
            bail!("{why}");
        }
        if self.helper.is_none() {
            line("Asking for administrator rights (once for this run).");
            match HelperClient::launch(self.sandbox.as_deref(), &self.log, self.launch) {
                Ok(h) => self.helper = Some(h),
                Err(e) => {
                    let why = format!("couldn't get administrator rights: {e:#}");
                    self.refused = Some(why.clone());
                    bail!(why);
                }
            }
        }
        let helper = self.helper.as_mut().expect("started above");
        helper.request(req, line).map(drop)
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
