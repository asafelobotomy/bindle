//! Run a plan for real: snapshot, then each step.
//!
//! The orchestrator runs as the user. Privileged steps go to one root helper
//! session (sudo on a terminal, pkexec otherwise); AUR builds and user
//! commands run here. Every file the plan touches is snapshotted first.
//! Nothing here prints: frontends render what [`Progress`] reports.

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Error, Result, anyhow};
use bindle_core::catalog::AssetKind;
use bindle_core::{Facts, Plan, Step};

use crate::client::{HelperClient, Launch};
use crate::protocol::{Request, SnapshotPlan};
use crate::{release, sys, user};

#[derive(Default)]
pub struct Options {
    /// Unattended: AUR helpers skip their review prompts.
    pub yes: bool,
    /// Run unprivileged against this directory, recording commands only.
    pub sandbox: Option<PathBuf>,
    /// How to start the root helper (the CLI's defaults: itself, sudo on a terminal).
    pub launch: Launch,
    /// Installing from the live ISO into the new system (`arch-chroot`): a
    /// user command that fails there (it may need a running session) is
    /// saved to this file and run at first login instead of failing the apply.
    pub defer_user: Option<PathBuf>,
}

impl Options {
    pub fn root(&self) -> PathBuf {
        self.sandbox.clone().unwrap_or_else(|| PathBuf::from("/"))
    }
}

/// What a running apply reports.
pub trait Progress {
    /// The pre-apply snapshot was saved under this id.
    fn snapshot(&mut self, id: &str);
    /// Step `index` (from 0) of `total` is starting.
    fn step(&mut self, index: usize, total: usize, step: &Step);
    /// A line of output from the root helper (pacman, systemctl, …).
    fn output(&mut self, line: &str);
    /// A line from a step run as the user (an AppImage installed, or what a
    /// sandbox run would have done).
    fn note(&mut self, line: &str);
}

/// How an apply that got as far as running steps ended.
pub struct Report {
    pub total: usize,
    /// The step that failed (from 0) and why; nothing after it ran.
    pub failed: Option<(usize, Error)>,
    pub snapshot: Option<String>,
    /// The log, as a path on this machine (inside the sandbox if there is one).
    pub log: PathBuf,
    /// A new kernel, microcode or graphics driver was installed.
    pub needs_reboot: bool,
    /// Group memberships changed; they apply from the next login.
    pub relogin: bool,
    /// Steps that did their part but left the user something to finish
    /// (module id, what to do), from a command's `manual` exit codes.
    pub manual: Vec<(String, String)>,
}

/// Run `plan`. Errors are setup failures (no root helper, no snapshot), when
/// nothing has changed; a failed step is reported in [`Report::failed`].
pub fn run(plan: &Plan, facts: &Facts, opts: &Options, progress: &mut dyn Progress) -> Result<Report> {
    let fake = opts.sandbox.is_some();
    let downloads = plan.steps.iter().any(|s| {
        matches!(
            s,
            Step::InstallPackages { .. }
                | Step::BootstrapAurHelper { .. }
                | Step::InstallAur { .. }
                | Step::InstallReleases { .. }
        )
    });
    if downloads
        && !fake
        && let Some((what, todo)) = crate::net::problem()
    {
        return Err(anyhow!("{what} {todo} Then run Bindle again; nothing was changed."));
    }
    let log = format!("/var/log/bindle/apply-{}.log", sys::now_id());
    let wants_root = plan.steps.iter().any(|s| {
        s.privileged()
            || matches!(s, Step::BootstrapAurHelper { .. })
            || matches!(s, Step::InstallReleases { assets }
                if assets.iter().any(|a| a.kind() == Some(AssetKind::Package)))
    });
    let mut helper =
        if wants_root { Some(HelperClient::launch(opts.sandbox.as_deref(), &log, &opts.launch)?) } else { None };

    let snapshot = match helper.as_mut() {
        Some(h) => {
            root_request(h, &Request::Snapshot { plan: snapshot_plan(plan, facts) }, &mut |l| progress.output(l))
                .map_err(|e| e.context("could not snapshot before applying; nothing was changed"))?
        }
        None => None,
    };
    if let Some(id) = &snapshot {
        progress.snapshot(id);
    }

    let aur_helper = facts.aur_helper.clone().unwrap_or_else(|| "paru".into());
    let total = plan.steps.len();
    let mut failed = None;
    let mut manual = Vec::new();
    for (i, step) in plan.steps.iter().enumerate() {
        progress.step(i, total, step);
        match run_step(step, helper.as_mut(), &aur_helper, opts.yes, fake, progress) {
            Err(e) if matches!(step, Step::RunAsUser { .. }) && opts.defer_user.is_some() => {
                let file = opts.defer_user.as_deref().unwrap_or(std::path::Path::new(""));
                if let Err(save) = defer(file, step) {
                    failed = Some((i, e.context(format!("and it couldn't be saved for first login: {save:#}"))));
                    break;
                }
                progress.note(&format!("{e:#}; it runs again at first login"));
            }
            Ok(None) => {}
            Ok(Some(todo)) => {
                progress.note(&format!("needs a manual step: {todo}"));
                let module = match step {
                    Step::RunAsUser { module, .. } => module.clone(),
                    _ => String::new(),
                };
                manual.push((module, todo));
            }
            Err(e) => {
                failed = Some((i, e));
                break;
            }
        }
    }
    if let Some(mut h) = helper {
        if failed.is_none()
            && let Some(id) = &snapshot
        {
            root_request(&mut h, &Request::Complete { id: id.clone() }, &mut |l| progress.output(l))?;
        }
        h.quit();
    }
    Ok(Report {
        total,
        failed,
        snapshot,
        log: sys::host(&opts.root(), &log),
        needs_reboot: needs_reboot(plan),
        relogin: plan.steps.iter().any(|s| matches!(s, Step::AddToGroups { .. })),
        manual,
    })
}

fn run_step(
    step: &Step,
    helper: Option<&mut HelperClient>,
    aur_helper: &str,
    unattended: bool,
    fake: bool,
    progress: &mut dyn Progress,
) -> Result<Option<String>> {
    if let Step::RunAsUser { argv, manual, .. } = step {
        return user::run_command(argv, manual, fake, &mut |l| progress.note(l));
    }
    let mut note = |l: &str| progress.note(l);
    match (step, helper) {
        (s, Some(h)) if s.privileged() => {
            root_request(h, &Request::Run { step: s.clone() }, &mut |l| progress.output(l)).map(drop)
        }
        (Step::BootstrapAurHelper { package }, Some(h)) => {
            let files = user::build_aur(package, fake, &mut note)?;
            install_files(h, files, progress)
        }
        (Step::InstallReleases { assets }, h) => {
            let files = release::install(assets, fake, &mut note)?;
            match h {
                Some(h) => install_files(h, files, progress),
                None if files.is_empty() => Ok(()),
                None => Err(anyhow!("no root helper to install release packages")),
            }
        }
        (Step::InstallAur { packages }, _) => user::install_aur(aur_helper, packages, unattended, fake, &mut note),
        (s, _) => Err(anyhow!("no root helper for {s:?}")),
    }
    .map(|()| None)
}

/// Append a user step to the first-login file (one JSON step per line).
fn defer(file: &std::path::Path, step: &Step) -> Result<()> {
    use std::io::Write;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(file)?;
    writeln!(f, "{}", serde_json::to_string(step)?)?;
    Ok(())
}

fn install_files(h: &mut HelperClient, files: Vec<String>, progress: &mut dyn Progress) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    root_request(h, &Request::InstallFiles { paths: files }, &mut |l| progress.output(l)).map(drop)
}

/// Whether the plan changes the kernel, microcode or drivers.
pub fn needs_reboot(plan: &Plan) -> bool {
    plan.steps.iter().any(|s| match s {
        Step::RebuildInitramfs => true,
        Step::InstallPackages { packages } => packages
            .iter()
            .any(|p| p.starts_with("linux") || p.ends_with("-ucode") || p.starts_with("nvidia") || p == "mesa"),
        _ => false,
    })
}

pub fn snapshot_plan(plan: &Plan, facts: &Facts) -> SnapshotPlan {
    let mut s = SnapshotPlan { label: plan.modules.join(", "), user: facts.user.clone(), ..Default::default() };
    for step in &plan.steps {
        match step {
            Step::EditPacmanConf { .. } => s.files.push("/etc/pacman.conf".into()),
            Step::WriteFile { file, .. } => s.files.push(file.path.clone()),
            Step::EnableUnits { units } => s.units.extend(units.iter().cloned()),
            Step::InstallPackages { packages } => s.packages.extend(packages.iter().cloned()),
            Step::RemovePackages { packages } => {
                s.manual.push(format!("removed {} (to restore: pacman -S {0})", packages.join(" ")))
            }
            Step::InstallAur { packages } => s.aur.extend(packages.iter().cloned()),
            Step::BootstrapAurHelper { package } => s.aur.push(package.clone()),
            Step::InstallReleases { assets } => {
                for a in assets {
                    match a.kind() {
                        Some(AssetKind::Package) => s.packages.push(a.name.clone()),
                        _ => s.manual.push(format!("installed ~/.local/bin/{} ({}); remove it by hand", a.name, a.tag)),
                    }
                }
            }
            Step::AddToGroups { user, groups } => {
                s.manual.push(format!("{user} added to groups {}", groups.join(", ")))
            }
            Step::RunAsRoot { argv, .. } | Step::RunAsUser { argv, .. } => {
                s.manual.push(format!("ran `{}`", sys::shell_words(argv)))
            }
            _ => {}
        }
    }
    let mut seen = BTreeSet::new();
    s.files.retain(|f| seen.insert(f.clone()));
    s
}

/// A request to the root helper with Ctrl-C ignored here: dying mid-step
/// would leave the helper (and pacman) running with nobody reporting.
pub fn root_request(h: &mut HelperClient, req: &Request, out: &mut dyn FnMut(&str)) -> Result<Option<String>> {
    sys::ignore_interrupts(true);
    let result = h.request(req, out);
    sys::ignore_interrupts(false);
    result
}

#[cfg(test)]
mod defer_tests {
    use super::*;

    #[test]
    fn deferred_user_steps_append_as_json_lines() {
        let dir = crate::sys::test_dir("defer");
        let file = dir.join("state/finish-at-login.jsonl");
        let step = |a: &str| Step::RunAsUser { module: "m".into(), argv: vec![a.into()], manual: Default::default() };
        defer(&file, &step("one")).unwrap();
        defer(&file, &step("two")).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        let back: Vec<Step> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(back, [step("one"), step("two")]);
    }
}
