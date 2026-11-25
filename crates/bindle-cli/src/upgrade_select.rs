//! bindleUP's one question: what to update. Asked once at the start of a
//! terminal run; after it (and the password) the run is unattended.

use std::collections::BTreeSet;
use std::io::{BufRead, Write};

use bindle_upgrade::{Stage, StagePlan, Todo};

/// A line of the list: a stage that would run, or an extra switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Stage(Stage),
    /// `fwupdmgr update`, not only listed.
    Firmware,
    /// Remove orphaned packages once the upgrade is done.
    Orphans,
}

#[derive(Debug, Clone)]
pub struct Choice {
    pub item: Item,
    pub label: String,
    pub on: bool,
}

/// What the user chose.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Picked {
    /// Stages left out.
    pub skip: BTreeSet<Stage>,
    pub firmware: bool,
    pub remove_orphans: bool,
}

/// The things this run can do, each on by default.
pub fn choices(plans: &[StagePlan], firmware_updates: bool) -> Vec<Choice> {
    let mut out: Vec<Choice> = plans
        .iter()
        .filter(|p| matches!(p.todo, Todo::Run(_)) && p.stage != Stage::Snapshot)
        .map(|p| Choice { item: Item::Stage(p.stage), label: format!("{}: {}", p.stage.title(), p.summary), on: true })
        .collect();
    if firmware_updates {
        out.push(Choice { item: Item::Firmware, label: "Firmware: install the updates fwupd lists".into(), on: true });
    }
    // Which packages are orphans is only known once the upgrade ran.
    out.push(Choice { item: Item::Orphans, label: "Clean up: remove orphaned packages afterwards".into(), on: true });
    out
}

/// Numbers (`1 3`, `1,3`) to flip; anything else is an error naming it.
pub fn parse_toggles(input: &str, n: usize) -> Result<Vec<usize>, String> {
    input
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .map(|w| match w.parse::<usize>() {
            Ok(i) if (1..=n).contains(&i) => Ok(i - 1),
            _ => Err(format!("`{w}` isn't one of 1-{n}")),
        })
        .collect()
}

pub fn picked(choices: &[Choice]) -> Picked {
    let mut p = Picked::default();
    for c in choices {
        match (&c.item, c.on) {
            (Item::Stage(s), false) => {
                p.skip.insert(*s);
            }
            (Item::Firmware, on) => p.firmware = on,
            (Item::Orphans, on) => p.remove_orphans = on,
            _ => {}
        }
    }
    p
}

fn show(choices: &[Choice], out: &mut dyn Write) {
    let _ = writeln!(out, "\nWhat should bindleup update?");
    for (i, c) in choices.iter().enumerate() {
        let _ = writeln!(out, "  {:>2}. [{}] {}", i + 1, if c.on { "x" } else { " " }, c.label);
    }
}

/// Ask until the user starts (`Some`) or quits (`None`). Nothing is asked
/// again after this: no y/n, no build review.
pub fn ask(mut choices: Vec<Choice>, input: &mut dyn BufRead, out: &mut dyn Write) -> Option<Picked> {
    loop {
        show(&choices, out);
        let _ = write!(out, "Enter starts; numbers switch items on or off (e.g. 2 4); q quits: ");
        let _ = out.flush();
        let mut line = String::new();
        if input.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim();
        if line.eq_ignore_ascii_case("q") {
            return None;
        }
        if line.is_empty() {
            if choices.iter().any(|c| c.on) {
                return Some(picked(&choices));
            }
            let _ = writeln!(out, "Nothing is switched on.");
            continue;
        }
        match parse_toggles(line, choices.len()) {
            Ok(ix) => ix.into_iter().for_each(|i| choices[i].on = !choices[i].on),
            Err(e) => {
                let _ = writeln!(out, "{e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Choice> {
        vec![
            Choice { item: Item::Stage(Stage::Pacman), label: "p".into(), on: true },
            Choice { item: Item::Stage(Stage::Aur), label: "a".into(), on: true },
            Choice { item: Item::Firmware, label: "f".into(), on: true },
            Choice { item: Item::Orphans, label: "o".into(), on: true },
        ]
    }

    #[test]
    fn toggles_parse() {
        assert_eq!(parse_toggles("1 3,4", 4).unwrap(), [0, 2, 3]);
        assert!(parse_toggles("5", 4).is_err());
        assert!(parse_toggles("x", 4).is_err());
        assert!(parse_toggles("0", 4).is_err());
        assert!(parse_toggles("", 4).unwrap().is_empty());
    }

    #[test]
    fn enter_takes_everything() {
        let p = ask(sample(), &mut "\n".as_bytes(), &mut Vec::new()).unwrap();
        assert_eq!(p, Picked { skip: BTreeSet::new(), firmware: true, remove_orphans: true });
    }

    #[test]
    fn toggled_items_are_left_out() {
        let p = ask(sample(), &mut "2 3\n\n".as_bytes(), &mut Vec::new()).unwrap();
        assert_eq!(p.skip, [Stage::Aur].into());
        assert!(!p.firmware && p.remove_orphans);
    }

    #[test]
    fn quitting_and_a_closed_input_start_nothing() {
        assert!(ask(sample(), &mut "q\n".as_bytes(), &mut Vec::new()).is_none());
        assert!(ask(sample(), &mut "".as_bytes(), &mut Vec::new()).is_none());
    }

    #[test]
    fn all_off_does_not_start() {
        let mut out = Vec::new();
        assert!(ask(sample(), &mut "1 2 3 4\n\nq\n".as_bytes(), &mut out).is_none());
        assert!(String::from_utf8(out).unwrap().contains("Nothing is switched on."));
    }

    #[test]
    fn choices_list_what_runs() {
        let plans = vec![
            StagePlan {
                stage: Stage::Pacman,
                todo: Todo::Run(vec![]),
                summary: "2 packages".into(),
                details: vec![],
                ask: None,
                stops: vec![],
                packages: vec![],
                upgrades: vec![],
            },
            StagePlan {
                stage: Stage::News,
                todo: Todo::Report,
                summary: "n".into(),
                details: vec![],
                ask: None,
                stops: vec![],
                packages: vec![],
                upgrades: vec![],
            },
        ];
        let c = choices(&plans, true);
        assert_eq!(
            c.iter().map(|c| c.item.clone()).collect::<Vec<_>>(),
            [Item::Stage(Stage::Pacman), Item::Firmware, Item::Orphans]
        );
    }
}
