//! What bindleupgrade prints: the `--check` report and the closing table.

use crate::plan::{StagePlan, Todo};
use crate::run::{Outcome, StageResult};

/// Detail lines shown per stage before "… and N more".
pub const MAX_DETAILS: usize = 12;

fn width() -> usize {
    crate::Stage::ALL.iter().map(|s| s.title().len()).max().unwrap_or(0)
}

/// Details of a stage, indented, cut at [`MAX_DETAILS`].
pub fn details(plan: &StagePlan) -> Vec<String> {
    let mut out: Vec<String> = plan.details.iter().take(MAX_DETAILS).map(|d| format!("    {d}")).collect();
    if plan.details.len() > MAX_DETAILS {
        out.push(format!("    … and {} more", plan.details.len() - MAX_DETAILS));
    }
    out
}

/// `--check`: one line per stage with what it would do, then its details.
pub fn check(plans: &[StagePlan]) -> Vec<String> {
    let w = width();
    let mut out = vec![];
    for p in plans {
        let mark = match &p.todo {
            Todo::Run(_) => "→",
            Todo::Report => "!",
            Todo::Nothing => "✓",
            Todo::Skip(_) => "–",
            Todo::Hold(_) => "‖",
        };
        let mut line = format!("{mark} {:<w$}  {}", p.stage.title(), p.summary);
        if let Todo::Hold(why) = &p.todo {
            line.push_str(&format!(" (held: {why})"));
        }
        out.push(line);
        if !matches!(p.todo, Todo::Skip(_) | Todo::Nothing) {
            out.extend(details(p));
        }
    }
    let todo = plans.iter().filter(|p| p.runs()).count();
    out.push(String::new());
    out.push(match todo {
        0 => "Nothing to update.".into(),
        n => format!("{n} stage(s) have work to do. Run `bindleupgrade` to do it."),
    });
    out
}

/// The closing table: one row per stage. A `rehearsal` (dry run, sandbox)
/// ran nothing, and says so.
pub fn table(results: &[StageResult], rehearsal: bool) -> Vec<String> {
    let w = width();
    let mut out = vec![format!("  {:<w$}  {:<11}  Details", "Stage", "Result")];
    for r in results {
        let (word, detail) = match &r.outcome {
            Outcome::Done if rehearsal => ("✓ rehearsed", r.summary.clone()),
            Outcome::Done => ("✓ done", r.summary.clone()),
            Outcome::UpToDate => ("✓ current", r.summary.clone()),
            Outcome::Reported => ("! note", r.summary.clone()),
            Outcome::Skipped(why) => ("– skipped", why.clone()),
            Outcome::Held(why) => ("‖ held", why.clone()),
            Outcome::Failed(why) => ("✗ failed", why.clone()),
        };
        out.push(format!("  {:<w$}  {word:<11}  {detail}", r.stage.title()));
    }
    out
}

pub fn any_failed(results: &[StageResult]) -> bool {
    results.iter().any(|r| matches!(r.outcome, Outcome::Failed(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Stage;
    use crate::plan::{Options, plan};
    use crate::probe::Probe;

    #[test]
    fn check_lists_every_stage_and_cuts_long_details() {
        let ups = (0..20).map(|i| crate::parse::Update { name: format!("p{i}"), old: "1".into(), new: "2".into() });
        let p = Probe { pacman: Ok(ups.collect()), ..Default::default() };
        let lines = check(&plan(&p, &Options::default()));
        assert!(lines.iter().any(|l| l.starts_with("→ Packages") && l.ends_with("20 packages to upgrade")));
        assert!(lines.contains(&"    … and 8 more".to_string()));
        assert_eq!(lines.last().unwrap(), "1 stage(s) have work to do. Run `bindleupgrade` to do it.");
        assert_eq!(lines.iter().filter(|l| !l.starts_with("    ") && !l.is_empty()).count(), Stage::ALL.len() + 1);
    }

    #[test]
    fn table_rows_line_up_and_show_failures() {
        let results = [
            StageResult { stage: Stage::Pacman, outcome: Outcome::Done, summary: "3 packages".into() },
            StageResult { stage: Stage::Aur, outcome: Outcome::Failed("paru failed".into()), summary: "x".into() },
            StageResult { stage: Stage::Firmware, outcome: Outcome::Skipped("--skip".into()), summary: "x".into() },
        ];
        let t = table(&results, false);
        assert!(table(&results, true)[1].contains("✓ rehearsed"));
        assert_eq!(t.len(), 4);
        assert!(t[2].contains("✗ failed") && t[2].ends_with("paru failed"));
        let col = |l: &str| l.find(['✓', '✗', '–']).unwrap();
        assert_eq!(col(&t[1]), col(&t[2]));
        assert!(any_failed(&results));
    }
}
