//! `bindle apply` / `bindle undo`: run a plan for real.
//!
//! The orchestrator runs as the user. Privileged steps go to one root helper
//! session (sudo on a terminal, pkexec otherwise); AUR builds and user
//! commands run here. Every file the plan touches is snapshotted first.

pub mod client;
pub mod helper;
pub mod protocol;
pub mod snapshot;
pub mod sys;
mod user;

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};
use bindle_core::{Facts, Plan, Step};

use crate::print;
use client::HelperClient;
use protocol::{Request, SnapshotPlan};

pub struct Options {
    pub yes: bool,
    /// Run unprivileged against this directory, recording commands only.
    pub sandbox: Option<PathBuf>,
}

impl Options {
    fn root(&self) -> PathBuf {
        self.sandbox.clone().unwrap_or_else(|| PathBuf::from("/"))
    }
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    NothingToDo,
    Cancelled,
    Applied,
}

pub fn confirm(question: &str, default_yes: bool) -> bool {
    print!("{question} {} ", if default_yes { "[Y/n]" } else { "[y/N]" });
    let _ = io::stdout().flush();
    let mut answer = String::new();
    if io::stdin().lock().read_line(&mut answer).is_err() {
        return false;
    }
    match answer.trim().to_lowercase().as_str() {
        "" => default_yes,
        a => a == "y" || a == "yes",
    }
}

fn snapshot_plan(plan: &Plan, facts: &Facts) -> SnapshotPlan {
    let mut s = SnapshotPlan { label: plan.modules.join(", "), user: facts.user.clone(), ..Default::default() };
    for step in &plan.steps {
        match step {
            Step::EditPacmanConf { .. } => s.files.push("/etc/pacman.conf".into()),
            Step::WriteFile { file, .. } => s.files.push(file.path.clone()),
            Step::EnableUnits { units } => s.units.extend(units.iter().cloned()),
            Step::InstallPackages { packages } => s.packages.extend(packages.iter().cloned()),
            Step::InstallAur { packages } => s.aur.extend(packages.iter().cloned()),
            Step::BootstrapAurHelper { package } => s.aur.push(package.clone()),
            Step::AddToGroups { user, groups } => {
                s.manual.push(format!("{user} added to groups {}", groups.join(", ")))
            }
            Step::RunAsRoot { argv, .. } | Step::RunAsUser { argv, .. } => {
                s.manual.push(format!("ran `{}`", sys::shell_words(argv)))
            }
            _ => {}
        }
    }
    s.files.dedup();
    s
}

fn needs_reboot(plan: &Plan) -> bool {
    plan.steps.iter().any(|s| match s {
        Step::RebuildInitramfs => true,
        Step::InstallPackages { packages } => packages
            .iter()
            .any(|p| p.starts_with("linux") || p.ends_with("-ucode") || p.starts_with("nvidia") || p == "mesa"),
        _ => false,
    })
}

pub fn apply(plan: &Plan, facts: &Facts, opts: &Options) -> Result<Outcome> {
    let fake = opts.sandbox.is_some();
    if !fake && sys::is_root() {
        bail!("run Bindle as your normal user; it asks for your password when it needs it");
    }
    print::plan_lines(plan).iter().for_each(|l| println!("{l}"));
    if plan.is_empty() {
        return Ok(Outcome::NothingToDo);
    }
    println!();
    if !opts.yes {
        if !io::stdin().is_terminal() {
            bail!("refusing to apply without a terminal; pass --yes to confirm non-interactively");
        }
        if !confirm("Apply these changes?", false) {
            return Ok(Outcome::Cancelled);
        }
    }

    let log = format!("/var/log/bindle/apply-{}.log", sys::now_id());
    let wants_root = plan.steps.iter().any(|s| s.privileged() || matches!(s, Step::BootstrapAurHelper { .. }));
    let mut helper = if wants_root { Some(HelperClient::start(opts.sandbox.as_deref(), &log)?) } else { None };
    let mut out = |line: &str| println!("    │ {line}");

    let snapshot = match helper.as_mut() {
        Some(h) => h
            .request(&Request::Snapshot { plan: snapshot_plan(plan, facts) }, &mut out)
            .map_err(|e| e.context("could not snapshot before applying; nothing was changed"))?,
        None => None,
    };
    if let Some(id) = &snapshot {
        println!("Snapshot {id} saved (undo with `bindle undo {id}`).");
    }

    let aur_helper = facts.aur_helper.clone().unwrap_or_else(|| "paru".into());
    let total = plan.steps.len();
    for (i, step) in plan.steps.iter().enumerate() {
        println!("\n[{}/{total}] {}", i + 1, print::describe(step));
        let result = match (step, helper.as_mut()) {
            (s, Some(h)) if s.privileged() => h.request(&Request::Run { step: s.clone() }, &mut out).map(drop),
            (Step::BootstrapAurHelper { package }, Some(h)) => user::build_aur(package, fake).and_then(|files| {
                if files.is_empty() {
                    Ok(())
                } else {
                    h.request(&Request::InstallFiles { paths: files }, &mut out).map(drop)
                }
            }),
            (Step::InstallAur { packages }, _) => user::install_aur(&aur_helper, packages, fake),
            (Step::RunAsUser { argv, .. }, _) => user::run(argv, fake),
            (s, None) => Err(anyhow::anyhow!("no root helper for {s:?}")),
            _ => unreachable!("privileged steps handled above"),
        };
        if let Err(e) = result {
            println!("  ✗ {e:#}");
            println!("\nStopped at step {}/{total}. Nothing after it ran.", i + 1);
            println!("Log: {}", sys::host(&opts.root(), &log).display());
            if let Some(id) = &snapshot {
                println!("Undo file changes: bindle undo {id}");
            }
            println!("Steps are idempotent: fix the cause and run Bindle again to continue.");
            if let Some(h) = helper {
                h.quit();
            }
            bail!("apply failed");
        }
        println!("  ✓ done");
    }
    if let Some(h) = helper {
        h.quit();
    }

    println!("\nAll {total} steps applied. Log: {}", sys::host(&opts.root(), &log).display());
    if needs_reboot(plan) {
        println!("Reboot to load the new kernel, microcode or drivers.");
    } else if plan.steps.iter().any(|s| matches!(s, Step::AddToGroups { .. })) {
        println!("Log out and back in for the new group memberships.");
    }
    Ok(Outcome::Applied)
}

pub fn undo(id: Option<String>, list: bool, opts: &Options) -> Result<()> {
    let root = opts.root();
    let snaps = snapshot::list(&root);
    if list || (snaps.is_empty() && id.is_none()) {
        if snaps.is_empty() {
            println!("No snapshots in {}.", sys::host(&root, snapshot::DIR).display());
        }
        for m in &snaps {
            println!(
                "{}  {}  {} files, {} units, {} packages  [{}]",
                m.id,
                m.created,
                m.files.len(),
                m.units.len(),
                m.packages.len() + m.aur.len(),
                m.label
            );
        }
        return Ok(());
    }
    let m = match id {
        Some(id) => snapshot::load(&root, &id)?,
        None => snaps.first().cloned().expect("checked above"),
    };
    println!("Undo snapshot {} ({}):", m.id, m.created);
    for f in &m.files {
        println!("  {} {}", if f.existed { "restore" } else { "remove " }, f.path);
    }
    if !m.units.is_empty() {
        println!("  systemctl disable --now {}", m.units.join(" "));
    }
    if !m.manual.is_empty() {
        println!("Not reverted (undo by hand if needed):");
        m.manual.iter().for_each(|c| println!("  {c}"));
    }
    if !opts.yes && !confirm("Proceed?", false) {
        return Ok(());
    }
    let log = format!("/var/log/bindle/undo-{}.log", sys::now_id());
    let mut helper = HelperClient::start(opts.sandbox.as_deref(), &log)?;
    helper.request(&Request::Restore { id: m.id.clone() }, &mut |l| println!("    │ {l}"))?;
    helper.quit();
    let pkgs: Vec<&String> = m.packages.iter().chain(&m.aur).filter(|p| still_installed(p, &root)).collect();
    if !pkgs.is_empty() {
        println!("\nPackages are left installed. To remove them:");
        println!("  sudo pacman -Rns {}", pkgs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "));
    }
    Ok(())
}

fn still_installed(pkg: &str, root: &Path) -> bool {
    root == Path::new("/") && Command::new("pacman").args(["-Q", pkg]).output().is_ok_and(|o| o.status.success())
}
