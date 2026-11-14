//! The update window's state, without Qt: what bindleupgrade found
//! (`bindle_upgrade::probe::Probe`), the stages switched off, and the
//! answers to the questions the terminal would ask. The plan comes from
//! `bindle_upgrade::plan`, the same pure planner the terminal uses; this
//! only turns it into what the Check page shows. `UpgradeBackend`
//! (`upgrade_backend.rs`) is the Qt bridge over it.

pub mod run;
pub mod sample;

use std::collections::{BTreeMap, BTreeSet};

use bindle_upgrade::plan::{Options, plan};
use bindle_upgrade::prefs::Prefs;
use bindle_upgrade::probe::Probe;
use bindle_upgrade::{Stage, StagePlan, Todo, news};
use serde_json::{Value, json};

/// What a stage's switch means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Switch {
    /// Off: the stage is left out (`--skip`).
    Skip,
    /// The answer to the stage's question (upgrade? remove orphans? read the news?).
    Answer,
    /// Firmware: install the updates, not only list them (`--firmware`).
    Firmware,
}

pub struct Upgrade {
    probe: Probe,
    prefs: Prefs,
    skip: BTreeSet<Stage>,
    answers: BTreeMap<Stage, bool>,
    firmware: bool,
}

impl Upgrade {
    pub fn new(probe: Probe, prefs: Prefs) -> Self {
        let mut u = Upgrade { probe, prefs, skip: BTreeSet::new(), answers: BTreeMap::new(), firmware: false };
        for p in u.display() {
            if let Some((_, default)) = p.ask {
                u.answers.insert(p.stage, default);
            }
        }
        u
    }

    pub fn probe(&self) -> &Probe {
        &self.probe
    }

    fn options(&self, skip: bool) -> Options {
        Options {
            skip: if skip { self.skip.clone() } else { BTreeSet::new() },
            firmware: self.firmware,
            windowed: true,
            prefs: self.prefs.clone(),
            ..Default::default()
        }
    }

    /// What each stage would do with every stage switched on.
    fn display(&self) -> Vec<StagePlan> {
        plan(&self.probe, &self.options(false))
    }

    /// The options the run plans with (the closing stages too).
    pub fn run_options(&self) -> Options {
        self.options(true)
    }

    /// The plan to run.
    pub fn plans(&self) -> Vec<StagePlan> {
        plan(&self.probe, &self.options(true))
    }

    /// The answers the run gives when a stage asks (`Ui::ask`).
    pub fn answers(&self) -> BTreeMap<Stage, bool> {
        self.answers.clone()
    }

    fn switch(&self, p: &StagePlan) -> Option<Switch> {
        let firmware = matches!(&self.probe.firmware, Some(Ok(l)) if !l.is_empty());
        if p.stage == Stage::Firmware && firmware {
            Some(Switch::Firmware)
        } else if p.ask.is_some() {
            Some(Switch::Answer)
        } else if p.runs() {
            Some(Switch::Skip)
        } else {
            None
        }
    }

    fn on(&self, p: &StagePlan, s: Switch) -> bool {
        match s {
            Switch::Skip => !self.skip.contains(&p.stage),
            Switch::Answer => self.answers.get(&p.stage).copied().unwrap_or(false),
            Switch::Firmware => self.firmware,
        }
    }

    /// Flip a stage's switch (by its id); false for an unknown or fixed stage.
    pub fn toggle(&mut self, id: &str) -> bool {
        let Ok(stage) = id.parse::<Stage>() else { return false };
        let Some(p) = self.display().into_iter().find(|p| p.stage == stage) else { return false };
        match self.switch(&p) {
            Some(Switch::Skip) if !self.skip.remove(&stage) => {
                self.skip.insert(stage);
            }
            Some(Switch::Skip) => {}
            Some(Switch::Answer) => {
                let a = self.answers.entry(stage).or_insert(false);
                *a = !*a;
            }
            Some(Switch::Firmware) => self.firmware = !self.firmware,
            None => return false,
        }
        true
    }

    /// Stages a question answered "no" holds back at run time (its `stops`),
    /// with why: the Check page shows them held, not as work.
    fn waiting(&self, run: &[StagePlan]) -> Vec<(Stage, String)> {
        let mut out: Vec<(Stage, String)> = vec![];
        for p in run.iter().filter(|p| p.ask.is_some() && !p.stops.is_empty()) {
            if self.answers.get(&p.stage).copied().unwrap_or(false) {
                continue;
            }
            let why = match p.stage {
                Stage::News => "held until you confirm you've read the news",
                Stage::Preflight => "held until you allow updating on battery",
                _ => "held: the system upgrade is switched off",
            };
            for s in &p.stops {
                if !out.iter().any(|(o, _)| o == s) {
                    out.push((*s, why.to_string()));
                }
            }
        }
        out
    }

    /// The AUR has updates that only a terminal can install.
    pub fn aur_in_terminal(&self) -> bool {
        matches!(&self.probe.aur, Ok(l) if !l.is_empty()) && self.probe.aur_helper.is_some()
    }

    /// The Check page: one row per stage, the new Arch news, and what the
    /// run costs.
    pub fn check(&self) -> Value {
        let display = self.display();
        let run = self.plans();
        let waiting = self.waiting(&run);
        let rows: Vec<Value> = display
            .iter()
            .zip(&run)
            .map(|(d, r)| {
                let switch = self.switch(d);
                let on = switch.is_none_or(|s| self.on(d, s));
                let wait = waiting.iter().find(|(s, _)| *s == d.stage).map(|(_, why)| why.clone());
                let held = match &r.todo {
                    Todo::Hold(why) => Some(why.clone()),
                    _ if r.runs() => wait.clone(),
                    _ => None,
                };
                let kind =
                    if held.is_some() { "hold" } else { kind(if switch == Some(Switch::Skip) && !on { d } else { r }) };
                json!({
                    "id": d.stage.id(),
                    "title": d.stage.title(),
                    "kind": kind,
                    "held": held,
                    "summary": d.summary,
                    "details": d.details,
                    "switch": switch.map(|s| label(d.stage, s)),
                    "on": on,
                })
            })
            .collect();
        let work = run.iter().filter(|p| p.runs() && !waiting.iter().any(|(s, _)| *s == p.stage)).count();
        let items = match &self.probe.news {
            Ok(items) => news::since(items, self.probe.last_upgrade, self.probe.today, self.prefs.news_days),
            Err(_) => vec![],
        };
        let news: Vec<Value> = items
            .iter()
            .map(|i| {
                json!({"date": bindle_upgrade::parse::ymd(i.day), "title": i.title, "link": i.link, "manual": i.manual()})
            })
            .collect();
        let reboot = run.iter().find(|p| p.stage == Stage::Reboot && p.todo == Todo::Report).map(|p| p.summary.clone());
        let restart_blocked = run.iter().find_map(|p| match &p.todo {
            Todo::Hold(why) if p.stage == Stage::Reboot => Some(why.clone()),
            _ => None,
        });
        json!({
            "state": "ready",
            "stages": rows,
            "news": news,
            "work": work,
            "newFeatures": self.probe.after.setup.whats_new.len(),
            "password": run.iter().any(StagePlan::needs_root),
            "reboot": reboot,
            "restartBlocked": restart_blocked,
            "snapper": self.probe.snapper_root,
            "snapshot": run.iter().any(|p| p.stage == Stage::Snapshot && p.runs()),
            "aurTerminal": self.aur_in_terminal(),
        })
    }
}

fn kind(p: &StagePlan) -> &'static str {
    match p.todo {
        Todo::Run(_) => "run",
        Todo::Report => "report",
        Todo::Nothing => "nothing",
        Todo::Skip(_) => "skip",
        Todo::Hold(_) => "hold",
    }
}

fn label(stage: Stage, s: Switch) -> &'static str {
    match (stage, s) {
        (Stage::News, _) => "I've read it and done what it says",
        (Stage::Preflight, _) => "Update on battery anyway",
        (Stage::Bindle, _) => "Update Bindle",
        (Stage::Rebuild, _) => "Rebuild them",
        (_, Switch::Firmware) => "Install them",
        (Stage::Orphans, _) => "Remove them",
        (Stage::Snapshot, _) => "Take one",
        (Stage::Mirrors, _) => "Refresh",
        _ => "Update",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upgrade() -> Upgrade {
        Upgrade::new(sample::probe(), Prefs::default())
    }

    fn row<'a>(check: &'a Value, id: &str) -> &'a Value {
        check["stages"].as_array().unwrap().iter().find(|r| r["id"] == id).unwrap()
    }

    #[test]
    fn every_stage_has_a_row_and_questions_start_at_their_defaults() {
        let u = upgrade();
        let c = u.check();
        assert_eq!(c["stages"].as_array().unwrap().len(), Stage::ALL.len());
        assert_eq!(row(&c, "pacman")["on"], true);
        assert_eq!(row(&c, "orphans")["on"], false, "removing orphans is opt-in");
        assert_eq!(row(&c, "news")["on"], false, "news asking for a manual step must be read first");
        assert_eq!(row(&c, "news")["switch"], "I've read it and done what it says");
        assert_eq!(row(&c, "firmware")["switch"], "Install them");
        assert_eq!(row(&c, "aur")["kind"], "report", "the window leaves the AUR to a terminal");
        assert_eq!(c["aurTerminal"], true);
        assert_eq!(c["password"], true);
        assert_eq!(c["news"][0]["manual"], true);
    }

    #[test]
    fn unread_news_shows_the_upgrades_held() {
        let mut u = upgrade();
        let c = u.check();
        for id in ["pacman", "releases"] {
            assert_eq!(row(&c, id)["kind"], "hold", "{id}");
            assert_eq!(row(&c, id)["held"], "held until you confirm you've read the news");
        }
        let held_work = c["work"].as_u64().unwrap();
        u.toggle("news");
        let c = u.check();
        assert_eq!(row(&c, "pacman")["kind"], "run");
        assert!(row(&c, "pacman")["held"].is_null());
        assert_eq!(c["work"].as_u64().unwrap(), held_work + 3, "the snapshot, packages and Bindle apps count again");
    }

    #[test]
    fn switching_off_the_system_upgrade_holds_what_needs_it() {
        let mut u = upgrade();
        u.toggle("news");
        u.toggle("pacman");
        let c = u.check();
        assert_eq!(row(&c, "pacman")["kind"], "run", "its own row shows what it would do, switched off");
        assert_eq!(row(&c, "releases")["held"], "held: the system upgrade is switched off");
    }

    #[test]
    fn switches_change_the_plan_to_run() {
        let mut u = upgrade();
        assert!(u.toggle("flatpak"));
        let c = u.check();
        assert_eq!(row(&c, "flatpak")["on"], false);
        assert_eq!(row(&c, "flatpak")["kind"], "run", "shown as what it would do");
        let flatpak = u.plans().into_iter().find(|p| p.stage == Stage::Flatpak).unwrap();
        assert!(matches!(flatpak.todo, Todo::Skip(_)));
        assert!(u.toggle("firmware"));
        assert!(u.plans().iter().any(|p| p.stage == Stage::Firmware && p.runs()));
        assert!(u.toggle("news"));
        assert!(u.answers()[&Stage::News]);
        assert!(!u.toggle("reboot"), "nothing to switch");
        assert!(!u.toggle("bogus"));
    }
}
