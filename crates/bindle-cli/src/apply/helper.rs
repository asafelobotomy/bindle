//! Root side of `bindle apply`: started once via sudo/pkexec, executes
//! privileged steps sent as JSON lines on stdin, answers with events on stdout.
//! In sandbox mode it runs unprivileged against a temp root and only records
//! the commands it would run.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use bindle_core::{Step, pacman_conf};

use super::protocol::{Event, Request, VERSION};
use super::snapshot;
use super::sys::{
    check_system_path, create_dirs, host, ignore_interrupts, is_root, lock_exclusive, run_streaming, shell_words,
    valid_name, write_atomic,
};

/// Held for the whole helper session: one apply or undo at a time.
const LOCK: &str = "/var/lib/bindle/lock";

pub struct HelperOpts {
    pub root: PathBuf,
    pub fake_commands: bool,
    pub log: String,
}

struct Helper {
    opts: HelperOpts,
    log: Option<fs::File>,
    out: io::Stdout,
    /// Snapshot of this session, for files that appear after it was taken.
    snapshot: Option<String>,
}

pub fn serve(opts: HelperOpts) -> Result<()> {
    if !opts.fake_commands && !is_root() {
        bail!("the helper must run as root");
    }
    check_system_path(&opts.log)?;
    let log_path = host(&opts.root, &opts.log);
    if let Some(dir) = log_path.parent() {
        create_dirs(dir)?;
    }
    let _lock = lock_exclusive(&host(&opts.root, LOCK))?;
    // A Ctrl-C meant for the terminal must not kill the helper mid-step.
    ignore_interrupts(true);
    // Root-only: command output can include whatever the commands print.
    // Never follow a symlink planted at the log path.
    let log = OpenOptions::new().create(true).append(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&log_path);
    let log_error = log.as_ref().err().map(|e| format!("warning: no log at {}: {e}", log_path.display()));
    let mut h = Helper { opts, log: log.ok(), out: io::stdout(), snapshot: None };
    h.emit(&Event::Hello { version: VERSION.into() });
    if let Some(warning) = log_error {
        h.emit(&Event::Output { line: warning });
    }
    for line in io::stdin().lock().lines() {
        let line = line?;
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                h.emit(&Event::Failed { error: format!("bad request: {e}") });
                continue;
            }
        };
        if matches!(req, Request::Quit) {
            break;
        }
        let result = h.handle(req);
        match result {
            Ok(Some(ev)) => h.emit(&ev),
            Ok(None) => h.emit(&Event::Done),
            Err(e) => {
                h.log_line(&format!("ERROR: {e:#}"));
                h.emit(&Event::Failed { error: format!("{e:#}") });
            }
        }
    }
    Ok(())
}

/// pacman.conf `[options]` keys are single words; values are one line. Anything
/// else could inject sections or directives.
fn check_pacman_edit(options: &BTreeMap<String, String>, repos: &[String]) -> Result<()> {
    for (k, v) in options {
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric()) || v.contains(['\n', '\r', '[', ']']) {
            bail!("refusing pacman.conf option `{k}`");
        }
    }
    match repos.iter().find(|r| r.is_empty() || !r.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
        Some(r) => bail!("refusing repo name `{r}`"),
        None => Ok(()),
    }
}

fn check_names(names: &[String]) -> Result<()> {
    match names.iter().find(|n| !valid_name(n)) {
        Some(bad) => bail!("refusing invalid name `{bad}`"),
        None => Ok(()),
    }
}

/// One log line per step; file contents are summarised, not copied into the log.
fn log_summary(step: &Step) -> String {
    match step {
        Step::WriteFile { module, file } => {
            format!("write {} (mode {:o}, {} bytes, module {module})", file.path, file.mode, file.content.len())
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

impl Helper {
    fn emit(&mut self, ev: &Event) {
        let _ = writeln!(self.out, "{}", serde_json::to_string(ev).unwrap_or_default());
        let _ = self.out.flush();
    }

    fn log_line(&mut self, line: &str) {
        if let Some(f) = &mut self.log {
            let _ = writeln!(f, "{line}");
        }
    }

    fn output(&mut self, line: &str) {
        self.log_line(line);
        self.emit(&Event::Output { line: line.to_string() });
    }

    fn capture_late(&self, path: &str) -> Result<()> {
        match &self.snapshot {
            Some(id) => snapshot::capture_late(self.root(), id, path),
            None => Ok(()),
        }
    }

    fn root(&self) -> &Path {
        &self.opts.root
    }

    fn cmd(&mut self, argv: &[&str]) -> Result<()> {
        let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        self.cmd_owned(&argv)
    }

    fn cmd_owned(&mut self, argv: &[String]) -> Result<()> {
        let shown = shell_words(argv);
        if self.opts.fake_commands {
            self.output(&format!("would run: {shown}"));
            return Ok(());
        }
        self.log_line(&format!("$ {shown}"));
        run_streaming(argv, &mut |l| self.output(l))
    }

    fn handle(&mut self, req: Request) -> Result<Option<Event>> {
        match req {
            Request::Snapshot { plan } => {
                for f in &plan.files {
                    check_system_path(f)?;
                }
                let m = snapshot::create(self.root(), &plan)?;
                self.log_line(&format!("snapshot {} ({} files)", m.id, m.files.len()));
                self.snapshot = Some(m.id.clone());
                Ok(Some(Event::Snapshot { id: m.id }))
            }
            Request::Run { step } => {
                self.log_line(&format!("== {}", log_summary(&step)));
                self.run_step(step).map(|()| None)
            }
            Request::InstallFiles { paths } => {
                for p in &paths {
                    check_system_path(p)?;
                    if !p.contains(".pkg.tar") || !Path::new(p).is_file() {
                        bail!("not a package file: {p}");
                    }
                }
                let mut argv = vec!["pacman".to_string(), "-U".into(), "--needed".into(), "--noconfirm".into()];
                argv.extend(paths);
                self.cmd_owned(&argv).map(|()| None)
            }
            Request::Restore { id } => {
                let m = snapshot::load(self.root(), &id)?;
                let report = snapshot::restore_files(self.root(), &m);
                for line in &report.done {
                    self.output(line);
                }
                let mut errors = report.errors;
                if !m.units.is_empty() {
                    let units = m.units.clone();
                    let disabled = check_names(&units).and_then(|()| {
                        let mut argv: Vec<String> = ["systemctl", "disable", "--now", "--"].map(String::from).into();
                        argv.extend(units);
                        self.cmd_owned(&argv)
                    });
                    if let Err(e) = disabled {
                        errors.push(format!("disable units: {e:#}"));
                    }
                }
                if !errors.is_empty() {
                    for e in &errors {
                        self.output(&format!("not restored: {e}"));
                    }
                    bail!("snapshot {id}: {} item(s) could not be restored (listed above)", errors.len());
                }
                Ok(None)
            }
            Request::Complete { id } => snapshot::mark_complete(self.root(), &id).map(|()| None),
            Request::Quit => Ok(None),
        }
    }

    fn run_step(&mut self, step: Step) -> Result<()> {
        if !step.privileged() {
            bail!("refusing non-privileged step in the root helper");
        }
        match step {
            Step::EditPacmanConf { options, enable_repos } => {
                check_pacman_edit(&options, &enable_repos)?;
                self.capture_late("/etc/pacman.conf")?;
                let path = host(self.root(), "/etc/pacman.conf");
                let mut text = fs::read_to_string(&path).context("read /etc/pacman.conf")?;
                for (k, v) in &options {
                    text = pacman_conf::set_option(&text, k, v);
                }
                for r in &enable_repos {
                    text = pacman_conf::enable_repo(&text, r);
                }
                let mode = fs::metadata(&path).map_or(0o644, |m| m.permissions().mode() & 0o7777);
                write_atomic(&path, text.as_bytes(), mode)?;
                self.output("updated /etc/pacman.conf");
                Ok(())
            }
            Step::InstallPackages { packages } => {
                check_names(&packages)?;
                let mut argv: Vec<String> =
                    ["pacman", "-Syu", "--needed", "--noconfirm", "--"].map(String::from).into();
                argv.extend(packages);
                self.cmd_owned(&argv)
            }
            Step::WriteFile { file, .. } => {
                check_system_path(&file.path)?;
                self.capture_late(&file.path)?;
                write_atomic(&host(self.root(), &file.path), file.content.as_bytes(), file.mode)?;
                self.output(&format!("wrote {}", file.path));
                Ok(())
            }
            Step::EnableUnits { units } => {
                check_names(&units)?;
                let mut argv: Vec<String> = ["systemctl", "enable", "--now", "--"].map(String::from).into();
                argv.extend(units);
                self.cmd_owned(&argv)
            }
            Step::AddToGroups { user, groups } => {
                if user == "root" || !valid_name(&user) || !groups.iter().all(|g| valid_name(g)) {
                    bail!("refusing group change for `{user}`");
                }
                self.cmd(&["usermod", "-aG", &groups.join(","), "--", &user])
            }
            Step::RunAsRoot { argv, .. } => self.cmd_owned(&argv),
            Step::RebuildInitramfs => self.cmd(&["mkinitcpio", "-P"]),
            Step::BootstrapAurHelper { .. } | Step::InstallAur { .. } | Step::RunAsUser { .. } => unreachable!(),
        }
    }
}
