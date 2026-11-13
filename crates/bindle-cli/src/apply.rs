//! `bindle apply` / `bindle undo` in the terminal: show the plan, ask, then
//! run it through `bindle_run::apply`, printing its progress.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};
use bindle_core::{Catalog, Facts, Plan, Selection, Step, plan};
use bindle_run::apply::{Progress, root_request};
use bindle_run::client::HelperClient;
use bindle_run::protocol::Request;

pub use bindle_run::apply::Options;
pub use bindle_run::{helper, snapshot, sys};

use crate::{print, selection, tui};

#[derive(Debug, PartialEq)]
pub enum Outcome {
    NothingToDo,
    Cancelled,
    Applied,
    /// A step failed after changes began; the cause has been printed.
    Failed,
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

/// Prints an apply's progress the way the terminal always has.
struct Printer;

impl Progress for Printer {
    fn snapshot(&mut self, id: &str) {
        println!("Snapshot {id} saved: `bindle undo` reverts this run.\n");
    }
    fn step(&mut self, index: usize, total: usize, step: &Step) {
        println!("[{}/{total}] {}", index + 1, print::describe(step));
    }
    fn output(&mut self, line: &str) {
        println!("    │ {line}");
    }
    fn note(&mut self, line: &str) {
        println!("    {line}");
    }
}

pub fn apply(plan: &Plan, facts: &Facts, opts: &Options) -> Result<Outcome> {
    if opts.sandbox.is_none() && sys::is_root() {
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
    let report = bindle_run::apply::run(plan, facts, opts, &mut Printer)?;
    let total = report.total;
    if let Some((i, e)) = report.failed {
        println!("  ✗ {e:#}");
        println!("\nStopped at step {}/{total}. Nothing after it ran.", i + 1);
        println!("Log: {}", report.log.display());
        if report.snapshot.is_some() {
            println!("Undo the changes so far: bindle undo");
        }
        println!("Steps are idempotent: fix the cause and run Bindle again to continue.");
        return Ok(Outcome::Failed);
    }
    println!("\nAll {total} steps applied. Log: {}", report.log.display());
    if report.snapshot.is_some() {
        println!("Undo with `bindle undo`.");
    }
    if report.needs_reboot {
        println!("Reboot to load the new kernel, microcode or drivers.");
    } else if report.relogin {
        println!("Log out and back in for the new group memberships.");
    }
    for (module, todo) in &report.manual {
        println!("Needs a manual step ({module}): {todo}");
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
            let mods: Vec<&str> = m.label.split(", ").collect();
            let label = match mods.len() {
                0..=4 => m.label.clone(),
                n => format!("{}, +{} more", mods[..3].join(", "), n - 3),
            };
            println!(
                "{}  {}  {} files, {} units, {} packages  [{label}]{}",
                m.id,
                m.created,
                m.files.len(),
                m.units.len(),
                m.packages.len() + m.aur.len(),
                if m.undone { "  (undone)" } else { "" },
            );
        }
        return Ok(());
    }
    // Newest first: each restore puts files back as they were before that run.
    let session = match id {
        Some(id) => vec![snapshot::load(&root, &id)?],
        None => snapshot::latest_session(&snaps),
    };
    if session.is_empty() {
        println!("Nothing left to undo: every snapshot has been undone (`bindle undo <id>` reverts one again).");
        return Ok(());
    }
    for m in &session {
        println!(
            "Undo snapshot {} ({}){}:",
            m.id,
            m.created,
            if m.complete { "" } else { ", an apply that stopped early" }
        );
        for f in &m.files {
            println!("  {} {}", if f.existed { "restore" } else { "remove " }, f.path);
        }
        if !m.units.is_empty() {
            println!("  systemctl disable --now {}", m.units.join(" "));
        }
        if !m.packages.is_empty() || !m.aur.is_empty() {
            println!(
                "  packages stay installed ({}); the command to remove them is printed at the end",
                m.packages.len() + m.aur.len()
            );
        }
        if !m.manual.is_empty() {
            println!("Not reverted (undo by hand if needed):");
            m.manual.iter().for_each(|c| println!("  {c}"));
        }
    }
    if !opts.yes && !confirm("Proceed?", false) {
        return Ok(());
    }
    let log = format!("/var/log/bindle/undo-{}.log", sys::now_id());
    let mut helper = HelperClient::start(opts.sandbox.as_deref(), &log)?;
    let mut failed = Vec::new();
    for m in &session {
        if let Err(e) =
            root_request(&mut helper, &Request::Restore { id: m.id.clone() }, &mut |l| println!("    │ {l}"))
        {
            println!("  ✗ {e:#}");
            failed.push(m.id.clone());
        }
    }
    helper.quit();
    let pkgs: Vec<&String> =
        session.iter().flat_map(|m| m.packages.iter().chain(&m.aur)).filter(|p| still_installed(p, &root)).collect();
    if !pkgs.is_empty() {
        println!("\nPackages are left installed. To remove them:");
        println!("  pkexec pacman -Rns {}", pkgs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "));
    }
    if !failed.is_empty() {
        bail!("undo incomplete for snapshot(s) {}; see the log", failed.join(", "));
    }
    println!("\nUndo complete.");
    Ok(())
}

fn still_installed(pkg: &str, root: &Path) -> bool {
    root == Path::new("/") && Command::new("pacman").args(["-Q", pkg]).output().is_ok_and(|o| o.status.success())
}

/// Written into the new system by the archinstall plugin.
pub const SYSTEM_SELECTION: &str = "/etc/bindle/selection.toml";

pub fn read_current(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Resolve, apply, and remember the selection once changes were made: also
/// when a step failed, so that running Bindle again (from any frontend)
/// continues with the same choices instead of an older selection.
pub fn apply_selection(catalog: &Catalog, facts: &Facts, sel: &Selection, opts: Options) -> Result<bool> {
    let current = |p: &str| match &opts.sandbox {
        Some(root) => std::fs::read_to_string(sys::host(root, p)).ok(),
        None => read_current(p),
    };
    let p = plan::resolve(catalog, facts, sel, &current)?;
    let outcome = apply(&p, facts, &opts)?;
    if matches!(outcome, Outcome::Applied | Outcome::Failed)
        && opts.sandbox.is_none()
        && let Some(path) = selection::default_path()
    {
        selection::save(sel, &path)?;
        println!("Selection saved to {} (replay with `bindle apply --from`).", path.display());
    }
    if outcome == Outcome::Failed {
        bail!("apply failed");
    }
    Ok(outcome != Outcome::Cancelled)
}

/// What the wizard starts from: the selection this user last applied (or
/// tried to), else the profile chosen before install (`bindle iso`).
pub fn seed_selection() -> Option<Selection> {
    let own = selection::default_path().and_then(|p| selection::load(&p).ok());
    own.or_else(|| selection::load(Path::new(SYSTEM_SELECTION)).ok())
}

/// Run the wizard; if the user chose to apply, apply. Returns whether the
/// user went through with it (applied, or nothing was left to do).
pub fn wizard(catalog: &Catalog, facts: &Facts, sandbox: Option<PathBuf>) -> Result<bool> {
    let seed = seed_selection();
    match tui::run(catalog, facts, seed)? {
        Some(sel) => apply_selection(catalog, facts, &sel, Options { yes: false, sandbox, ..Default::default() }),
        None => Ok(false),
    }
}
