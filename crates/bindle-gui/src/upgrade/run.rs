//! Running the update from the window: `bindle_upgrade::run::run` on a
//! worker thread, each stage and output line turned into the Update page's
//! state. Questions a stage asks are answered by the switches on the Check
//! page; anything else (go on without a snapshot?) is answered no.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use bindle_run::client::{Launch, NO_AGENT};
use bindle_upgrade::run::{Outcome, RunOpts, StageResult, Ui};
use bindle_upgrade::{Stage, StagePlan, report};
use serde::Serialize;

/// Lines of command output the page keeps (the log file has them all).
const LOG_LINES: usize = 400;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunState {
    /// idle, running, done (every stage ran or was left out), failed (one
    /// or more stages failed) or error (nothing ran).
    pub state: String,
    pub stages: Vec<StageRow>,
    pub log: Vec<String>,
    /// Every line of the run, readable by the user (the helper's own log is root's).
    pub log_path: String,
    pub reboot: Option<String>,
    /// Restarting isn't safe yet (a DKMS module or boot image missing), with the fix.
    pub restart_blocked: Option<String>,
    /// New Bindle features this machine would get: offer to open Bindle.
    pub new_features: usize,
    /// Bindle replaced itself: the new one does the rest (offer to reopen).
    pub relaunch: bool,
    /// The AUR has updates: offer to run them in a terminal.
    pub aur_terminal: bool,
    /// The system upgrade failed: offer to run it in a terminal, where pacman can ask.
    pub retry_terminal: bool,
    /// pkexec found no polkit agent: offer the terminal instead.
    pub no_agent: bool,
    pub error: String,
    /// A dry run or sandbox: nothing was changed.
    pub rehearsal: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StageRow {
    pub id: String,
    pub title: String,
    pub summary: String,
    /// pending, running, ran (finished, result not in yet), done, current
    /// (up to date), note, skipped, held or failed.
    pub status: String,
    pub detail: String,
}

impl RunState {
    pub fn starting(plans: &[StagePlan], rehearsal: bool) -> Self {
        let stages = plans
            .iter()
            .map(|p| StageRow {
                id: p.stage.id().into(),
                title: p.stage.title().into(),
                summary: p.summary.clone(),
                status: "pending".into(),
                detail: String::new(),
            })
            .collect();
        RunState { state: "running".into(), stages, rehearsal, ..Default::default() }
    }

    /// Final statuses from the results, and what to offer next.
    fn finish(&mut self, results: &[StageResult]) {
        for r in results {
            let Some(row) = self.stages.iter_mut().find(|s| s.id == r.stage.id()) else { continue };
            row.summary.clone_from(&r.summary);
            let (status, detail) = match &r.outcome {
                Outcome::Done => ("done", String::new()),
                Outcome::UpToDate => ("current", String::new()),
                Outcome::Reported => ("note", String::new()),
                // The window's answers are its switches.
                Outcome::Skipped(why) if why == "you said no" => ("skipped", "switched off".into()),
                Outcome::Skipped(why) => ("skipped", why.clone()),
                Outcome::Held(why) => ("held", why.clone()),
                Outcome::Failed(why) => ("failed", why.clone()),
            };
            row.status = status.into();
            row.detail = detail;
        }
        let failed: Vec<&StageResult> = results.iter().filter(|r| matches!(r.outcome, Outcome::Failed(_))).collect();
        self.no_agent = failed.iter().any(|r| matches!(&r.outcome, Outcome::Failed(m) if m.contains(NO_AGENT)));
        self.retry_terminal = failed.iter().any(|r| r.stage == Stage::Pacman) || self.no_agent;
        // Restart: recommended, blocked (a DKMS module or boot image is
        // missing and wasn't built), or fine again once the fix ran.
        let reboot = results.iter().find(|r| r.stage == Stage::Reboot);
        let unsafe_now = |summary: &str| summary.split("; building").next().unwrap_or(summary).to_string();
        (self.reboot, self.restart_blocked) = match reboot.map(|r| (&r.outcome, r.summary.as_str())) {
            Some((Outcome::Reported, summary)) => (Some(summary.to_string()), None),
            Some((Outcome::Held(why), _)) => (None, Some(why.clone())),
            Some((Outcome::Done, _)) => (Some("the missing modules and boot images are built".into()), None),
            Some((Outcome::Failed(_) | Outcome::Skipped(_), summary)) if summary.starts_with("don't restart yet") => {
                (None, Some(unsafe_now(summary)))
            }
            _ => (None, None),
        };
        self.relaunch = bindle_upgrade::run::relaunch(results, self.rehearsal);
        self.state = if failed.is_empty() { "done" } else { "failed" }.into();
    }
}

/// The window's side of a run: publishes a fresh state on every change.
struct Window<F: FnMut(&RunState)> {
    state: RunState,
    answers: BTreeMap<Stage, bool>,
    current: Option<Stage>,
    all: Vec<String>,
    publish: F,
}

impl<F: FnMut(&RunState)> Ui for Window<F> {
    fn stage(&mut self, _index: usize, _total: usize, plan: &StagePlan) {
        for row in &mut self.state.stages {
            if row.status == "running" {
                row.status = "ran".into();
            }
            if row.id == plan.stage.id() {
                row.status = "running".into();
                row.summary.clone_from(&plan.summary);
            }
        }
        self.current = Some(plan.stage);
        self.all.push(format!("==> {}: {}", plan.stage.title(), plan.summary));
        (self.publish)(&self.state);
    }

    fn line(&mut self, line: &str) {
        self.all.push(format!("    {line}"));
        if self.state.log.len() == LOG_LINES {
            self.state.log.remove(0);
        }
        self.state.log.push(line.to_string());
        (self.publish)(&self.state);
    }

    /// A stage's own question: its switch. Any other (remove unused
    /// runtimes? go on without a snapshot?): its default answer.
    fn ask(&mut self, question: &str, default: bool) -> bool {
        let answer = self.current.and_then(|s| self.answers.get(&s).copied()).unwrap_or(default);
        self.all.push(format!("    {question} {}", if answer { "yes" } else { "no" }));
        answer
    }
}

/// How the window runs: a rehearsal prints commands or records them.
pub struct How {
    pub dry_run: bool,
    pub sandbox: Option<PathBuf>,
    pub aur_terminal: bool,
    pub new_features: usize,
}

/// Run `plans` and publish the state as it changes; returns the final state.
pub fn run(
    plans: Vec<StagePlan>,
    answers: BTreeMap<Stage, bool>,
    after: &mut bindle_upgrade::run::Closing,
    how: How,
    publish: impl FnMut(&RunState),
) -> RunState {
    let rehearsal = how.dry_run || how.sandbox.is_some();
    let mut w = Window { state: RunState::starting(&plans, rehearsal), answers, current: None, all: vec![], publish };
    w.state.aur_terminal = how.aur_terminal;
    w.state.new_features = how.new_features;
    let exe = crate::apply::helper_exe();
    if exe.is_none() && !how.dry_run {
        w.state.state = "error".into();
        w.state.error = "The bindle program isn't installed next to this window, so it can't make changes.".into();
        return w.state;
    }
    let opts = RunOpts {
        yes: false,
        dry_run: how.dry_run,
        sandbox: how.sandbox,
        launch: Launch { exe, graphical: true },
        tty: None,
    };
    let results = bindle_upgrade::run::run(plans, after, &opts, &mut w);
    w.state.finish(&results);
    w.all.push(String::new());
    w.all.extend(report::table(&results, rehearsal));
    w.state.log_path = save_log(&w.all).unwrap_or_default();
    w.state
}

/// The whole run, in `~/.cache/bindle/upgrade-<time>.log`.
fn save_log(lines: &[String]) -> Option<String> {
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?
        .join("bindle");
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("upgrade-{}.log", bindle_run::sys::now_id()));
    fs::write(&path, lines.join("\n") + "\n").ok()?;
    Some(path.display().to_string())
}

/// `bindleupgrade` in a terminal (all of it, or `--only aur`), where pacman
/// and paru can ask. A rehearsal passes its `--dry-run` / `--sandbox` on.
pub fn in_terminal(only_aur: bool, dry_run: bool, sandbox: Option<&str>) -> anyhow::Result<()> {
    let exe = crate::apply::helper_exe().ok_or_else(|| anyhow::anyhow!("the bindle program isn't installed"))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["in-terminal", "upgrade"]);
    if only_aur {
        // uQ's setup and the rebuilds follow an AUR update.
        cmd.args(["--only", "aur,uq,rebuild"]);
    }
    if dry_run {
        cmd.arg("--dry-run");
    }
    if let Some(dir) = sandbox {
        cmd.arg("--sandbox").arg(dir);
    }
    cmd.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upgrade::{Upgrade, sample};
    use bindle_upgrade::plan::after_plans;
    use bindle_upgrade::prefs::Prefs;

    #[test]
    fn a_dry_run_follows_the_switches() {
        let mut u = Upgrade::new(sample::probe(), Prefs::default());
        u.toggle("news"); // read it
        let after_data = u.probe().after.clone();
        let mut seen = 0;
        let mut after = |up: &[String], _: Option<Vec<String>>| {
            after_plans(&after_data, &after_data, up, &Default::default(), false)
        };
        let how = How { dry_run: true, sandbox: None, aur_terminal: u.aur_in_terminal(), new_features: 1 };
        let end = run(u.plans(), u.answers(), &mut after, how, |_| seen += 1);
        let status = |id: &str| end.stages.iter().find(|s| s.id == id).unwrap().status.clone();
        assert_eq!(status("pacman"), "done");
        assert_eq!(status("aur"), "note", "left to the terminal");
        assert_eq!(status("orphans"), "skipped", "its switch starts off");
        assert_eq!(end.stages.iter().find(|s| s.id == "orphans").unwrap().detail, "switched off");
        assert_eq!(end.state, "done");
        assert!(end.rehearsal && end.aur_terminal);
        assert!(end.log.iter().any(|l| l == "would run as root: pacman -Syu --noconfirm"));
        assert_eq!(end.reboot.as_deref(), Some("recommended: updated linux, mesa"));
        assert!(seen > 5);
    }

    #[test]
    fn unread_news_holds_the_upgrade() {
        let u = Upgrade::new(sample::probe(), Prefs::default());
        let how = How { dry_run: true, sandbox: None, aur_terminal: false, new_features: 0 };
        let end = run(u.plans(), u.answers(), &mut |_, _| vec![], how, |_| {});
        let pacman = end.stages.iter().find(|s| s.id == "pacman").unwrap();
        assert_eq!(pacman.status, "held");
        assert_eq!(pacman.detail, "you stopped to read the news first");
    }

    #[test]
    fn a_restart_is_fine_again_once_the_fix_ran() {
        let u = Upgrade::new(sample::held(), Prefs::default());
        let after_data = u.probe().after.clone();
        let mut after =
            |up: &[String], _: Option<Vec<String>>| after_plans(&after_data, &after_data, up, &u.run_options(), false);
        let how = How { dry_run: true, sandbox: None, aur_terminal: false, new_features: 0 };
        let end = run(u.plans(), u.answers(), &mut after, how, |_| {});
        assert!(end.log.iter().any(|l| l == "would run as root: dkms autoinstall -k 7.3.5-zen1-1-zen"));
        assert_eq!(end.reboot.as_deref(), Some("the missing modules and boot images are built"));
        assert!(end.restart_blocked.is_none());
    }
}
