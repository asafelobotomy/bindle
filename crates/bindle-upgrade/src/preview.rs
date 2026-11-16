//! What a run would change, as the plan document uQ Settings shows before
//! "Update now" (`bindleup --dry-run --json`). Pure, from the stage plans.

use bindle_core::preview::PlanDoc;

use crate::plan::{StagePlan, Todo};
use crate::stage::Stage;

/// One change per detail line of each stage that runs (its summary when it
/// has none), then the stages held back. Nothing to run and something held:
/// the update would be refused, with the reasons.
pub fn document(plans: &[StagePlan]) -> PlanDoc {
    let mut doc = PlanDoc::default();
    let mut held = vec![];
    for p in plans {
        match &p.todo {
            Todo::Run(_) => {
                let group = group(p);
                if p.details.is_empty() {
                    doc.change(&group, p.summary.clone(), "", None);
                }
                for d in &p.details {
                    doc.change(&group, d.clone(), "", None);
                }
            }
            Todo::Hold(why) => held.push((p, why.clone())),
            _ => {}
        }
    }
    let running = plans.iter().filter(|p| p.runs()).count();
    for (p, why) in &held {
        doc.change("Held back", p.stage.title(), why.clone(), Some(true));
    }
    doc.summary = match running {
        0 => "Nothing to update".to_string(),
        1 => "1 stage will run".to_string(),
        n => format!("{n} stages will run"),
    };
    if running == 0 && !held.is_empty() {
        let why: Vec<String> = held.iter().map(|(p, w)| format!("{}: {w}", p.stage.title())).collect();
        return doc.refuse(why.join(". "));
    }
    doc
}

/// The group a stage's changes are shown under. The AUR says what is
/// different about it: built without the helper's review.
fn group(p: &StagePlan) -> String {
    let base = p.stage.title();
    match p.stage {
        Stage::Aur => format!("{base} (built without review)"),
        _ if p.needs_root() => format!("{base} (asks for your password)"),
        _ => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Update;
    use crate::plan::{Options, plan};
    use crate::probe::Probe;

    #[test]
    fn a_pending_upgrade_lists_each_package() {
        let ups = vec![Update { name: "a".into(), old: "1".into(), new: "2".into() }];
        let p = Probe { pacman: Ok(ups), ..Default::default() };
        let d = document(&plan(&p, &Options::default()));
        assert!(!d.would_refuse);
        assert!(d.changes.iter().any(|c| c.group.starts_with("Packages") && c.label.contains('a')));
    }

    #[test]
    fn nothing_to_do_is_not_a_refusal() {
        let d = document(&plan(&Probe::default(), &Options::default()));
        assert_eq!(d.summary, "Nothing to update");
        assert!(!d.would_refuse);
    }
}
