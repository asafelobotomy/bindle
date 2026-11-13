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

/// Let pacman itself parse the edited file before it replaces the real one.
fn check_pacman_conf(text: &str) -> Result<()> {
    let tmp = Path::new("/etc/.pacman.conf.bindle-check");
    write_atomic(tmp, text.as_bytes(), 0o600)?;
    let out = std::process::Command::new("pacman-conf").arg("--config").arg(tmp).arg("--repo-list").output();
    let _ = fs::remove_file(tmp);
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            bail!("edited pacman.conf does not parse; left unchanged: {}", String::from_utf8_lossy(&o.stderr).trim())
        }
        // No pacman-conf (shouldn't happen on Arch): nothing to check with.
        Err(_) => Ok(()),
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
                self.cmd_owned(&argv).map(|()| None)
            }
            Request::Restore { id } => {
                let m = snapshot::load(self.root(), &id)?;
                let report = snapshot::restore_files(self.root(), &m);
                for line in &report.done {
                    self.output(line);
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
                snapshot::mark_undone(self.root(), &id).map(|()| None)
            }
            Request::Log { line } => {
                // One line per request, whatever it contains.
                self.log_line(&line.replace(['\n', '\r'], " "));
                Ok(None)
            }
            Request::Complete { id } => snapshot::mark_complete(self.root(), &id).map(|()| None),
            Request::Quit => Ok(None),
        }
    }

    /// Copy user-built package files into a root-owned directory, so they
    /// can't change between the checks here and pacman reading them.
    fn stage_packages(&self, paths: &[String]) -> Result<Vec<String>> {
        let dir = host(self.root(), STAGING);
        let _ = fs::remove_dir_all(&dir);
        create_dirs(&dir)?;
        let mut out = Vec::new();
        for p in paths {
            check_system_path(p)?;
            let name = Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let regular = fs::symlink_metadata(p).is_ok_and(|m| m.is_file());
            if !name.contains(".pkg.tar") || name.starts_with('.') || !regular {
                bail!("not a package file: {p}");
            }
            let data = fs::read(p).with_context(|| format!("read {p}"))?;
            write_atomic(&dir.join(&name), &data, 0o644)?;
            out.push(format!("{STAGING}/{name}"));
        }
        Ok(out)
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
                if !self.opts.fake_commands {
                    check_pacman_conf(&text)?;
                }
                let mode = fs::metadata(&path).map_or(0o644, |m| m.permissions().mode() & 0o7777);
                write_atomic(&path, text.as_bytes(), mode)?;
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
                self.capture_late(&file.path)?;
                write_atomic(&host(self.root(), &file.path), file.content.as_bytes(), file.mode)?;
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
        self.capture_late(fstab::PATH)?;
        let path = host(self.root(), fstab::PATH);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e).context("read /etc/fstab"),
        };
        let mode = fs::metadata(&path).map_or(0o644, |m| m.permissions().mode() & 0o7777);
        write_atomic(&path, fstab::merge(&text, entries).as_bytes(), mode)?;
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
