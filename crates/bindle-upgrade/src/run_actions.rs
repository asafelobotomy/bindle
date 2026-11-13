//! The executor's actions: root commands through the helper, user
//! commands, release downloads, Bindle's own update and uQ's setup.

use std::process::Command;

use anyhow::{Context, Result, bail};
use bindle_core::Step;
use bindle_run::client::HelperClient;
use bindle_run::protocol::{BindleFile, Request};
use bindle_run::{release, sys};

use super::Exec;
use crate::plan::Action;
use crate::selfupdate::{self, Location};

impl Exec<'_> {
    pub(super) fn action(&mut self, action: &Action) -> Result<()> {
        match action {
            Action::Root(argv) => {
                self.root(Request::Run { step: Step::RunAsRoot { module: "bindleupgrade".into(), argv: argv.clone() } })
            }
            Action::RootTerminal(argv) => {
                let tty = self.opts.tty.clone().context("no terminal to run it on")?;
                // pacman (as root) ignores Ctrl-C mid-transaction; so does this side.
                sys::ignore_interrupts(true);
                let done = self.root(Request::Interactive { argv: argv.clone(), tty });
                sys::ignore_interrupts(false);
                done
            }
            Action::User(argv) => self.user(argv),
            Action::Releases(assets) => {
                let fake = self.rehearsal();
                let ui = &mut *self.ui;
                let files = release::install(assets, fake, &mut |l| ui.line(l))?;
                if files.is_empty() { Ok(()) } else { self.root(Request::InstallFiles { paths: files }) }
            }
            Action::Optional { question, default, action } => {
                if self.opts.yes || self.ui.ask(question, *default) {
                    self.action(action)
                } else {
                    Ok(())
                }
            }
            Action::SelfUpdate { tag, location } => self.self_update(tag, location),
            Action::UqSetup => self.uq_setup(),
        }
    }

    /// /boot's file names through the root helper, when this run started it.
    pub(super) fn boot_files(&mut self) -> Option<Vec<String>> {
        let helper = self.helper.as_mut()?;
        let mut names = vec![];
        helper.request(&Request::BootFiles, &mut |l| names.push(l.to_string())).ok()?;
        Some(names)
    }

    fn rehearsal(&self) -> bool {
        self.opts.dry_run || self.opts.sandbox.is_some()
    }

    /// Download and verify the release, then replace this Bindle: the
    /// AppImage in place, the installed one through the helper (which checks
    /// each file's sha256 again on its own copy).
    fn self_update(&mut self, tag: &str, location: &Location) -> Result<()> {
        if self.rehearsal() {
            self.ui.line(&format!(
                "would download Bindle {tag}, check SHA256SUMS's signature (key {}) and each file's sha256",
                selfupdate::SIGNING_FPR
            ));
            return Ok(());
        }
        let cache = std::env::var_os("XDG_CACHE_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
            .context("no cache directory")?;
        self.ui.line(&format!("Downloading Bindle {tag} and checking its signature"));
        let files = selfupdate::download(tag, location, &cache.join("bindle/self").join(tag))?;
        match location {
            Location::AppImage(path) => {
                let (file, _, _) = files.first().context("no AppImage in the release")?;
                selfupdate::replace_appimage(file, std::path::Path::new(path))?;
            }
            Location::Installed { .. } => {
                let files = files
                    .into_iter()
                    .map(|(src, dest, sha256)| BindleFile { src: src.display().to_string(), dest, sha256 })
                    .collect();
                self.root(Request::InstallBindle { files })?;
            }
            Location::Elsewhere(path) => bail!("{path} isn't a Bindle this can replace"),
        }
        self.ui.line(&format!("Bindle {tag} is installed; it goes on from here."));
        Ok(())
    }

    /// `uqctl setup --apps --json`: 0 uQ's config is in place, 3 the user's
    /// own hyprland.lua was kept (not a failure; they add uQ's stub).
    fn uq_setup(&mut self) -> Result<()> {
        let argv = ["uqctl", "setup", "--apps", "--json"];
        if self.rehearsal() {
            self.ui.line(&format!("would run: {}", argv.join(" ")));
            return Ok(());
        }
        let out = Command::new(argv[0]).args(&argv[1..]).output().context("start uqctl")?;
        let code = out.status.code().unwrap_or(-1);
        match crate::system::uq_setup(&String::from_utf8_lossy(&out.stdout)) {
            Ok(lines) if code == 0 || code == 3 => {
                lines.iter().for_each(|l| self.ui.line(l));
                if code == 3 {
                    self.ui.line("Your own hyprland.lua was kept: add uQ's lines to it (uqctl setup prints them).");
                }
                Ok(())
            }
            Ok(_) => bail!("uqctl setup failed (exit {code})"),
            Err(e) => bail!("uqctl setup: {e}"),
        }
    }

    pub(super) fn root(&mut self, req: Request) -> Result<()> {
        if self.opts.dry_run {
            let shown = match &req {
                Request::Run { step: Step::RunAsRoot { argv, .. } } => sys::shell_words(argv),
                Request::Interactive { argv, .. } => format!("{} (on this terminal)", sys::shell_words(argv)),
                other => format!("{other:?}"),
            };
            self.ui.line(&format!("would run as root: {shown}"));
            return Ok(());
        }
        if let Some(why) = &self.refused {
            bail!("{why}");
        }
        if self.helper.is_none() {
            self.ui.line("Asking for administrator rights (once for this run).");
            match HelperClient::launch(self.opts.sandbox.as_deref(), &self.log, &self.opts.launch) {
                Ok(h) => self.helper = Some(h),
                Err(e) => {
                    let why = format!("couldn't get administrator rights: {e:#}");
                    self.refused = Some(why.clone());
                    bail!(why);
                }
            }
        }
        let helper = self.helper.as_mut().expect("started above");
        let ui = &mut *self.ui;
        helper.request(&req, &mut |l| ui.line(l)).map(drop)
    }

    pub(super) fn user(&mut self, argv: &[String]) -> Result<()> {
        let shown = sys::shell_words(argv);
        if self.rehearsal() {
            self.ui.line(&format!("would run: {shown}"));
            return Ok(());
        }
        if let Some(h) = self.helper.as_mut() {
            let _ = h.request(&Request::Log { line: format!("(as the user) $ {shown}") }, &mut |_| {});
        }
        // Without a terminal (the window), the output is shown line by line.
        if self.opts.tty.is_none() {
            let ui = &mut *self.ui;
            return sys::run_streaming(argv, &mut |l| ui.line(l));
        }
        let (prog, args) = argv.split_first().context("empty command")?;
        let status = Command::new(prog).args(args).status().with_context(|| format!("start `{prog}`"))?;
        if !status.success() {
            bail!("`{shown}` failed ({status})");
        }
        Ok(())
    }
}
