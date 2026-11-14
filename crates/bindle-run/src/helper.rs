//! Root side of `bindle apply`: started once via sudo/pkexec, executes
//! privileged steps sent as JSON lines on stdin, answers with events on stdout.
//! In sandbox mode it runs unprivileged against a temp root and only records
//! the commands it would run.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use bindle_core::fstab::{self, Entry};
use bindle_core::{Step, pacman_conf};

use crate::protocol::{Event, Request, VERSION};
use crate::snapshot;
use crate::sys::{
    check_system_path, create_dirs, host, ignore_interrupts, is_root, lock_exclusive, run_streaming, shell_words,
    valid_name, write_atomic,
};

/// Held for the whole helper session: one apply or undo at a time.
const LOCK: &str = "/var/lib/bindle/lock";
/// Root-owned copies of AUR packages built as the user.
const STAGING: &str = "/var/cache/bindle/pkg";
/// The only directory `--log` may name a file in (opened as root).
const LOG_DIR: &str = "/var/log/bindle/";

mod stage;

pub struct HelperOpts {
    pub root: PathBuf,
    pub fake_commands: bool,
    pub log: String,
}

struct Helper {
    opts: HelperOpts,
    log: Option<fs::File>,
    /// Events go here: stdout, or a buffer in tests.
    out: Box<dyn Write>,
    /// Snapshot of this session, for files that appear after it was taken.
    snapshot: Option<String>,
}

pub fn serve(opts: HelperOpts) -> Result<()> {
    if !opts.fake_commands && !is_root() {
        bail!("the helper must run as root");
    }
    check_log_path(&opts.log)?;
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
    let mut h = Helper { opts, log: log.ok(), out: Box::new(io::stdout()), snapshot: None };
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

/// A file directly in `LOG_DIR`: root opens it, so nowhere else.
fn check_log_path(log: &str) -> Result<()> {
    check_system_path(log)?;
    match log.strip_prefix(LOG_DIR) {
        Some(name) if !name.is_empty() && !name.contains('/') && !name.starts_with('.') => Ok(()),
        _ => bail!("the helper's log goes in {LOG_DIR}, not `{log}`"),
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

    /// `snapshot::capture_late` (before a write) or `record_written` (after).
    fn snap(&self, f: fn(&Path, &str, &str) -> Result<()>, path: &str) -> Result<()> {
        match &self.snapshot {
            Some(id) => f(self.root(), id, path),
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
                for f in plan.files.iter().chain(&plan.dirs) {
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
                let copies = self.stage_packages(&paths)?;
                let mut argv =
                    vec!["pacman".to_string(), "-U".into(), "--needed".into(), "--noconfirm".into(), "--".into()];
                argv.extend(copies);
                let result = self.cmd_owned(&argv);
                // pacman has read them (or failed): the copies have no further use.
                let _ = fs::remove_dir_all(host(self.root(), STAGING));
                result.map(|()| None)
            }
            Request::Restore { id, force } => {
                let m = snapshot::load(self.root(), &id)?;
                let report = snapshot::restore_files(self.root(), &m, force);
                for line in report.done.iter().chain(&report.kept) {
                    self.output(line);
                }
                if !report.kept.is_empty() {
                    self.output(&format!("to put the originals back anyway: bindle undo --force {id}"));
                }
                let mut errors = report.errors;
                // systemd made mount units from the fstab that was just put back.
                if m.files.iter().any(|f| f.path == fstab::PATH)
                    && let Err(e) = self.cmd(&["systemctl", "daemon-reload"])
                {
                    errors.push(format!("reload systemd: {e:#}"));
                }
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
                snapshot::mark_undone(self.root(), &id)?;
                crate::record::on_undone(self.root(), &id).map(|()| None)
            }
            Request::Interactive { argv, tty } => {
                self.log_line(&format!("$ {}  (on {tty})", shell_words(&argv)));
                let fake = self.opts.fake_commands;
                crate::tty::serve(&argv, &tty, fake, &mut |l| self.output(l)).map(|()| None)
            }
            Request::BootFiles => {
                for name in crate::pacman_check::boot_files(self.root())? {
                    self.emit(&Event::Output { line: name });
                }
                Ok(None)
            }
            Request::InstallBindle { files } => {
                let (root, fake) = (self.opts.root.clone(), self.opts.fake_commands);
                crate::self_install::install(&root, &files, fake, &mut |l| self.output(l)).map(|()| None)
            }
            Request::Log { line } => {
                // One line per request, whatever it contains.
                self.log_line(&line.replace(['\n', '\r'], " "));
                Ok(None)
            }
            Request::Complete { id } => {
                snapshot::mark_complete(self.root(), &id)?;
                crate::record::on_complete(self.root(), &snapshot::load(self.root(), &id)?).map(|()| None)
            }
            Request::Quit => Ok(None),
        }
    }

    fn run_step(&mut self, step: Step) -> Result<()> {
        if !step.privileged() {
            bail!("refusing non-privileged step in the root helper");
        }
        match step {
            Step::EditPacmanConf { options, enable_repos } => {
                crate::pacman_check::edit(&options, &enable_repos)?;
                self.snap(snapshot::capture_late, "/etc/pacman.conf")?;
                let path = host(self.root(), "/etc/pacman.conf");
                let mut text = fs::read_to_string(&path).context("read /etc/pacman.conf")?;
                for (k, v) in &options {
                    text = pacman_conf::set_option(&text, k, v);
                }
                for r in &enable_repos {
                    text = pacman_conf::enable_repo(&text, r);
                }
                if !self.opts.fake_commands {
                    crate::pacman_check::conf(&text)?;
                }
                let mode = fs::metadata(&path).map_or(0o644, |m| m.permissions().mode() & 0o7777);
                write_atomic(&path, text.as_bytes(), mode)?;
                self.snap(snapshot::record_written, "/etc/pacman.conf")?;
                self.output("updated /etc/pacman.conf");
                Ok(())
            }
            Step::RemovePackages { packages } => {
                check_names(&packages)?;
                // -dd: dependents are satisfied again by the install step
                // right after, which brings the replacements' `provides`.
                let mut argv: Vec<String> = ["pacman", "-Rdd", "--noconfirm", "--"].map(String::from).into();
                argv.extend(packages);
                self.cmd_owned(&argv)
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
                self.snap(snapshot::capture_late, &file.path)?;
                write_atomic(&host(self.root(), &file.path), file.content.as_bytes(), file.mode)?;
                self.snap(snapshot::record_written, &file.path)?;
                self.output(&format!("wrote {}", file.path));
                Ok(())
            }
            Step::EnableUnits { units } => {
                check_names(&units)?;
                // In the new system from the ISO nothing can start yet: enable only.
                let now = if crate::sys::in_chroot() { None } else { Some("--now") };
                let mut argv: Vec<String> =
                    ["systemctl", "enable"].into_iter().chain(now).chain(["--"]).map(String::from).collect();
                argv.extend(units);
                self.cmd_owned(&argv)
            }
            Step::DisableUnits { units } => {
                check_names(&units)?;
                let now = if crate::sys::in_chroot() { None } else { Some("--now") };
                let mut argv: Vec<String> =
                    ["systemctl", "disable"].into_iter().chain(now).chain(["--"]).map(String::from).collect();
                argv.extend(units);
                self.cmd_owned(&argv)
            }
            Step::RemoveFiles { paths } => {
                for p in &paths {
                    check_system_path(p)?;
                    let target = host(self.root(), p);
                    // Only what Bindle wrote, and never through a symlink.
                    let ours = fs::symlink_metadata(&target).is_ok_and(|m| m.is_file())
                        && fs::read_to_string(&target).is_ok_and(|t| t.starts_with(bindle_core::plan::retire::MARKER));
                    if !ours {
                        self.output(&format!("kept {p}: not a file Bindle wrote"));
                        continue;
                    }
                    fs::remove_file(&target).with_context(|| format!("remove {p}"))?;
                    self.snap(snapshot::record_written, p)?;
                    self.output(&format!("removed {p}"));
                }
                Ok(())
            }
            Step::MountAtBoot { entries } => self.mount_at_boot(&entries),
            Step::AddToGroups { user, groups } => {
                if user == "root" || !valid_name(&user) || !groups.iter().all(|g| valid_name(g)) {
                    bail!("refusing group change for `{user}`");
                }
                self.cmd(&["usermod", "-aG", &groups.join(","), "--", &user])
            }
            Step::RunAsRoot { argv, .. } => self.cmd_owned(&argv),
            Step::RebuildInitramfs => self.cmd(&["mkinitcpio", "-P"]),
            Step::BootstrapAurHelper { .. }
            | Step::InstallAur { .. }
            | Step::InstallReleases { .. }
            | Step::RunAsUser { .. } => unreachable!(),
        }
    }

    /// Put the drives' lines in Bindle's fstab block, create their mount
    /// points and mount what isn't mounted yet. A drive that won't mount now
    /// doesn't fail the apply: its line is `nofail` and is tried at startup.
    fn mount_at_boot(&mut self, entries: &[Entry]) -> Result<()> {
        for e in entries {
            e.check().map_err(anyhow::Error::msg)?;
            check_system_path(&e.dir)?;
        }
        self.snap(snapshot::capture_late, fstab::PATH)?;
        let path = host(self.root(), fstab::PATH);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e).context("read /etc/fstab"),
        };
        let mode = fs::metadata(&path).map_or(0o644, |m| m.permissions().mode() & 0o7777);
        write_atomic(&path, fstab::merge(&text, entries).as_bytes(), mode)?;
        self.snap(snapshot::record_written, fstab::PATH)?;
        for e in entries {
            create_dirs(&host(self.root(), &e.dir))?;
            self.output(&format!("added to /etc/fstab: {}", e.line().replace('\t', " ")));
        }
        // In the new system from the ISO there is no systemd to tell yet.
        if crate::sys::in_chroot() {
            return Ok(());
        }
        self.cmd(&["systemctl", "daemon-reload"])?;
        for e in entries.iter().filter(|e| e.mount_now) {
            if let Err(err) = self.cmd(&["mount", "--", &e.dir]) {
                self.output(&format!("could not mount {} now ({err:#}); it is tried again at every start", e.dir));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
