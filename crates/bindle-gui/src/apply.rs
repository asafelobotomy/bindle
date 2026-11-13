//! Running the plan from the window: `bindle_run::apply` on a worker thread,
//! with every progress event turned into the Apply screen's state.

use std::path::{Path, PathBuf};

use bindle_core::{Facts, Plan, Selection, Step};
use bindle_run::apply::{Options, Progress};
use bindle_run::client::{Launch, NO_AGENT};
use bindle_run::{present, selection};
use serde::Serialize;

/// Set once a plan applied; the exit code tells `bindle first-login run`.
pub static APPLIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Lines of command output the screen keeps.
const LOG_LINES: usize = 300;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyState {
    /// idle, running, done, failed (a step failed) or error (nothing ran).
    pub state: String,
    pub steps: Vec<StepState>,
    pub log: Vec<String>,
    pub snapshot: Option<String>,
    pub error: String,
    pub log_path: String,
    pub needs_reboot: bool,
    pub relogin: bool,
    /// pkexec had no polkit agent to ask for the password: offer a terminal.
    pub no_agent: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepState {
    pub text: String,
    /// pending, running, done or failed.
    pub status: String,
}

impl ApplyState {
    pub fn starting(plan: &Plan) -> Self {
        let steps = plan.steps.iter().map(|s| StepState { text: present::describe(s), status: "pending".into() });
        ApplyState { state: "running".into(), steps: steps.collect(), ..Default::default() }
    }

    fn push_log(&mut self, line: String) {
        if self.log.len() == LOG_LINES {
            self.log.remove(0);
        }
        self.log.push(line);
    }
}

/// The `bindle` binary that serves as the root helper: next to this program
/// (installed together), else on PATH.
pub fn helper_exe() -> Option<PathBuf> {
    let sibling = std::env::current_exe().ok()?.with_file_name("bindle");
    if sibling.is_file() {
        return Some(sibling);
    }
    std::env::var_os("PATH")?.to_str()?.split(':').map(|d| Path::new(d).join("bindle")).find(|p| p.is_file())
}

/// Reports progress by handing a fresh copy of the state to `publish`.
struct Reporter<F: FnMut(&ApplyState)> {
    state: ApplyState,
    publish: F,
}

impl<F: FnMut(&ApplyState)> Progress for Reporter<F> {
    fn snapshot(&mut self, id: &str) {
        self.state.snapshot = Some(id.to_string());
        (self.publish)(&self.state);
    }
    fn step(&mut self, index: usize, _total: usize, _step: &Step) {
        for (i, s) in self.state.steps.iter_mut().enumerate() {
            s.status = if i < index {
                "done"
            } else if i == index {
                "running"
            } else {
                "pending"
            }
            .into();
        }
        (self.publish)(&self.state);
    }
    fn output(&mut self, line: &str) {
        self.state.push_log(line.to_string());
        (self.publish)(&self.state);
    }
    fn note(&mut self, line: &str) {
        self.state.push_log(line.to_string());
        (self.publish)(&self.state);
    }
}

/// Apply `plan` and publish the state as it changes; returns the final state.
pub fn run(
    plan: &Plan,
    facts: &Facts,
    sel: &Selection,
    sandbox: Option<PathBuf>,
    publish: impl FnMut(&ApplyState),
) -> ApplyState {
    let mut r = Reporter { state: ApplyState::starting(plan), publish };
    let Some(exe) = helper_exe() else {
        r.state.state = "error".into();
        r.state.error = "The bindle program isn't installed next to this window, so it can't make changes.".into();
        return r.state;
    };
    // The review screen was the confirmation: AUR helpers run unattended.
    let opts = Options {
        yes: true,
        sandbox: sandbox.clone(),
        launch: Launch { exe: Some(exe), graphical: true },
        ..Default::default()
    };
    match bindle_run::apply::run(plan, facts, &opts, &mut r) {
        Err(e) => {
            r.state.state = "error".into();
            r.state.no_agent = e.to_string() == NO_AGENT;
            r.state.error = format!("{e:#}");
        }
        Ok(report) => {
            r.state.log_path = report.log.display().to_string();
            r.state.needs_reboot = report.needs_reboot;
            r.state.relogin = report.relogin;
            // Like `bindle apply`: remember the selection once changes were
            // made, also after a failed step, so a retry starts from it.
            if sandbox.is_none()
                && let Some(path) = selection::default_path()
            {
                let _ = selection::save(sel, &path);
            }
            match report.failed {
                Some((i, e)) => {
                    r.state.state = "failed".into();
                    r.state.error = format!("{e:#}");
                    for (j, s) in r.state.steps.iter_mut().enumerate() {
                        s.status = if j < i {
                            "done"
                        } else if j == i {
                            "failed"
                        } else {
                            "pending"
                        }
                        .into();
                    }
                }
                None => {
                    r.state.state = "done".into();
                    APPLIED.store(true, std::sync::atomic::Ordering::SeqCst);
                    r.state.steps.iter_mut().for_each(|s| s.status = "done".into());
                }
            }
        }
    }
    r.state
}

/// No polkit agent: save the selection and finish in a terminal, where sudo
/// can ask for the password.
pub fn continue_in_terminal(sel: &Selection) -> anyhow::Result<()> {
    let exe = helper_exe().ok_or_else(|| anyhow::anyhow!("the bindle program isn't installed"))?;
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .ok_or_else(|| anyhow::anyhow!("no cache directory"))?
        .join("bindle");
    let file = dir.join("window-selection.toml");
    selection::save(sel, &file)?;
    std::process::Command::new(exe).arg("in-terminal").arg("apply").arg("--from").arg(&file).spawn()?;
    Ok(())
}

/// `bindle undo` in a terminal, where it can ask for the password and
/// show what it puts back. `sandbox`: the hidden `--sandbox` of a test run.
pub fn undo_in_terminal(sandbox: Option<&str>) -> anyhow::Result<()> {
    let exe = helper_exe().ok_or_else(|| anyhow::anyhow!("the bindle program isn't installed"))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("in-terminal").arg("undo");
    if let Some(dir) = sandbox {
        cmd.arg("--sandbox").arg(dir);
    }
    cmd.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_moves_through_the_steps() {
        let plan = Plan {
            steps: vec![Step::RebuildInitramfs, Step::EnableUnits { units: vec!["a.service".into()] }],
            ..Default::default()
        };
        let mut seen = Vec::new();
        let mut r = Reporter { state: ApplyState::starting(&plan), publish: |s: &ApplyState| seen.push(s.clone()) };
        r.step(1, 2, &plan.steps[1]);
        r.output("hello");
        assert_eq!(r.state.steps[0].status, "done");
        assert_eq!(r.state.steps[1].status, "running");
        assert_eq!(r.state.log, ["hello"]);
        for i in 0..LOG_LINES + 5 {
            r.note(&i.to_string());
        }
        assert_eq!(r.state.log.len(), LOG_LINES);
        drop(r);
        assert!(seen.len() > 2);
    }
}
