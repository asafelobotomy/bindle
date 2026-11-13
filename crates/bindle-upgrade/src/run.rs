//! Carry out a plan, stage by stage. Root commands go through one session
//! of Bindle's root helper (started only when the first one is due, so one
//! password prompt); AUR helpers, Flatpak and fwupd run as the user with
//! the terminal, as they expect. A failed stage doesn't stop the run, but
//! stages that depend on it are skipped: no AUR builds on top of a failed
//! system upgrade.

use std::collections::BTreeMap;
use std::path::PathBuf;

use bindle_run::client::{HelperClient, Launch};
use bindle_run::sys;

use crate::plan::{StagePlan, Todo};
use crate::stage::Stage;

/// What a frontend shows and asks.
pub trait Ui {
    /// Stage `index` (from 0) of `total` begins.
    fn stage(&mut self, index: usize, total: usize, plan: &StagePlan);
    /// A line from a command or about one.
    fn line(&mut self, line: &str);
    fn ask(&mut self, question: &str, default: bool) -> bool;
}

#[derive(Default)]
pub struct RunOpts {
    /// Questions take their "yes" answer.
    pub yes: bool,
    /// Print every command, run none, start no root helper.
    pub dry_run: bool,
    /// Root commands go to a helper that only records them, against this
    /// directory; user commands are printed.
    pub sandbox: Option<PathBuf>,
    pub launch: Launch,
    /// The user's terminal, for commands that ask questions.
    pub tty: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    UpToDate,
    Reported,
    Skipped(String),
    Held(String),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct StageResult {
    pub stage: Stage,
    pub outcome: Outcome,
    pub summary: String,
}

/// Plans the closing stages from the packages upgraded and /boot's files.
pub type Closing<'a> = dyn FnMut(&[String], Option<Vec<String>>) -> Vec<StagePlan> + 'a;

/// Run the stages of `plans` before the upgrade ends, then ask `after` for
/// the closing stages and run those. `after` is given the packages upgraded,
/// and the files in /boot when the root helper could list them (/boot is
/// often root-only).
pub fn run(plans: Vec<StagePlan>, after: &mut Closing, opts: &RunOpts, ui: &mut dyn Ui) -> Vec<StageResult> {
    let log = format!("/var/log/bindle/upgrade-{}.log", sys::now_id());
    let mut ex = Exec { opts, ui, helper: None, refused: None, log, blocked: BTreeMap::new(), updated: vec![] };
    let total = Stage::ALL.len();
    let mut results: Vec<StageResult> = vec![];
    let rehearsal = opts.dry_run || opts.sandbox.is_some();
    for plan in plans.into_iter().filter(|p| !p.stage.after_upgrade()) {
        if relaunch(&results, rehearsal) {
            // The new Bindle runs the rest, with its own pins and stages.
            let why = "the new Bindle goes on from here".to_string();
            results.push(StageResult { stage: plan.stage, outcome: Outcome::Skipped(why), summary: plan.summary });
            continue;
        }
        results.push(ex.stage(plan, results.len(), total));
    }
    if relaunch(&results, rehearsal) {
        if let Some(h) = ex.helper.take() {
            h.quit();
        }
        return results;
    }
    let updated = ex.updated.clone();
    let boot = ex.boot_files();
    for plan in after(&updated, boot) {
        results.push(ex.stage(plan, results.len(), total));
    }
    if let Some(h) = ex.helper.take() {
        h.quit();
    }
    results
}

/// Bindle replaced itself in this (real) run: the caller starts the new one
/// with `selfupdate::UPDATED_ENV` set, to do the rest.
pub fn relaunch(results: &[StageResult], rehearsal: bool) -> bool {
    !rehearsal && results.iter().any(|r| r.stage == Stage::Bindle && r.outcome == Outcome::Done)
}

struct Exec<'a> {
    opts: &'a RunOpts,
    ui: &'a mut dyn Ui,
    helper: Option<HelperClient>,
    /// The helper couldn't start (password dismissed, no polkit agent):
    /// later root steps fail with this instead of asking again.
    refused: Option<String>,
    log: String,
    /// Stages that won't run, and why.
    blocked: BTreeMap<Stage, String>,
    /// Packages upgraded so far.
    updated: Vec<String>,
}

impl Exec<'_> {
    fn stage(&mut self, plan: StagePlan, index: usize, total: usize) -> StageResult {
        self.ui.stage(index, total, &plan);
        let result = |outcome| StageResult { stage: plan.stage, outcome, summary: plan.summary.clone() };
        if plan.runs()
            && let Some(why) = self.blocked.get(&plan.stage)
        {
            return result(Outcome::Held(why.clone()));
        }
        let actions = match &plan.todo {
            Todo::Skip(_) => return result(Outcome::Skipped(plan.summary.clone())),
            Todo::Hold(why) => return result(Outcome::Held(why.clone())),
            Todo::Nothing => return result(Outcome::UpToDate),
            Todo::Report => {
                if !self.agreed(&plan) {
                    let why = match plan.stage {
                        Stage::Preflight => "you chose to wait for mains power",
                        _ => "you stopped to read the news first",
                    };
                    self.block(&plan.stops, why);
                }
                return result(Outcome::Reported);
            }
            Todo::Run(actions) => actions,
        };
        if !self.agreed(&plan) {
            self.block(&plan.stops, "you stopped the upgrade");
            return result(Outcome::Skipped("you said no".into()));
        }
        for action in actions {
            if let Err(e) = self.action(action) {
                let msg = format!("{e:#}");
                self.ui.line(&format!("✗ {msg}"));
                self.failed(plan.stage);
                return result(Outcome::Failed(msg));
            }
        }
        self.updated.extend(plan.packages.iter().cloned());
        result(Outcome::Done)
    }

    fn agreed(&mut self, plan: &StagePlan) -> bool {
        match &plan.ask {
            Some(_) if self.opts.yes => true,
            Some((q, default)) => self.ui.ask(q, *default),
            None => true,
        }
    }

    fn block(&mut self, stages: &[Stage], why: &str) {
        for s in stages {
            self.blocked.entry(*s).or_insert_with(|| why.to_string());
        }
    }

    /// What a failure means for the stages after it.
    fn failed(&mut self, stage: Stage) {
        match stage {
            Stage::Pacman => {
                if self.opts.tty.is_none() || self.opts.yes {
                    self.ui.line("pacman may have needed an answer: run bindleupgrade in a terminal, without --yes.");
                }
                self.block(&[Stage::Aur, Stage::Releases], "the system upgrade failed; fix that first")
            }
            Stage::Snapshot => {
                let go_on = self.opts.yes || self.ui.ask("No snapshot was taken. Upgrade anyway?", false);
                if !go_on {
                    self.block(&[Stage::Pacman, Stage::Aur, Stage::Releases], "no snapshot was taken");
                }
            }
            _ => {}
        }
    }
}

#[path = "run_actions.rs"]
mod actions;

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
