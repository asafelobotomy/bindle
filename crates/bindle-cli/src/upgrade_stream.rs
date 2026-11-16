//! `bindleup --ndjson`: the check and the run as one JSON object per line,
//! for uQ Settings' Updates tab (docs/GUI.md, "uQ integration"). Events:
//!
//! - check: `step` (what is being looked at), then one `plan` with every stage.
//! - run: `stage` (index, total, id, title, summary), `line` (a command's
//!   output), `ask` (a question and the answer it took), then a `result` per
//!   stage and `done`. After Bindle replaced itself, `relaunch`: the new
//!   one's events follow, from the first stage.

use std::io::Write;

use bindle_upgrade::run::{Outcome, StageResult, Ui};
use bindle_upgrade::{StagePlan, Todo};
use serde_json::{Value, json};

pub fn emit(v: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn state(t: &Todo) -> &'static str {
    match t {
        Todo::Run(_) => "run",
        Todo::Report => "note",
        Todo::Nothing => "current",
        Todo::Skip(_) => "skipped",
        Todo::Hold(_) => "held",
    }
}

/// The whole plan, once the system has been looked at.
pub fn plan_event(plans: &[StagePlan]) {
    let stages: Vec<Value> = plans
        .iter()
        .map(|p| {
            let why = match &p.todo {
                Todo::Skip(w) | Todo::Hold(w) => w.as_str(),
                _ => "",
            };
            json!({
                "id": p.stage.id(), "title": p.stage.title(), "state": state(&p.todo),
                "summary": p.summary, "why": why, "details": p.details, "needs_root": p.needs_root(),
            })
        })
        .collect();
    let work = plans.iter().filter(|p| p.runs()).count();
    emit(&json!({ "event": "plan", "stages": stages, "work": work }));
}

pub fn result_event(r: &StageResult) {
    let (outcome, why) = match &r.outcome {
        Outcome::Done => ("done", String::new()),
        Outcome::UpToDate => ("current", String::new()),
        Outcome::Reported => ("note", String::new()),
        Outcome::Skipped(w) => ("skipped", w.clone()),
        Outcome::Held(w) => ("held", w.clone()),
        Outcome::Failed(w) => ("failed", w.clone()),
    };
    emit(
        &json!({ "event": "result", "id": r.stage.id(), "title": r.stage.title(), "outcome": outcome, "summary": r.summary, "why": why }),
    );
}

/// A run's progress as events; every question takes its default answer
/// (the run is unattended).
pub struct Stream;

impl Ui for Stream {
    fn stage(&mut self, index: usize, total: usize, p: &StagePlan) {
        emit(
            &json!({ "event": "stage", "index": index, "total": total, "id": p.stage.id(), "title": p.stage.title(), "summary": p.summary }),
        );
    }

    fn line(&mut self, line: &str) {
        emit(&json!({ "event": "line", "text": line }));
    }

    fn ask(&mut self, question: &str, default: bool) -> bool {
        emit(&json!({ "event": "ask", "question": question, "answer": default }));
        default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bindle_upgrade::Stage;

    #[test]
    fn questions_take_their_default() {
        assert!(Stream.ask("Go on?", true));
        assert!(!Stream.ask("Remove orphans?", false));
    }

    #[test]
    fn states_name_each_todo() {
        assert_eq!(state(&Todo::Hold("x".into())), "held");
        assert_eq!(state(&Todo::Nothing), "current");
        assert_eq!(Stage::Aur.id(), "aur");
    }
}
