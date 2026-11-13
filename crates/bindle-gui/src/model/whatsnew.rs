//! "What's new" in the window: this Bindle compared with what earlier
//! versions set up (`bindle_run::whatsnew`), the user's picks, and while it
//! is open, the plan and selection the Review and Apply pages use: exactly
//! the picks, then the saved selection updated by them.

use std::path::Path;

use bindle_core::whatsnew::{Change, Item, Kind, WhatsNew};
use bindle_core::{Plan, Selection};
use bindle_run::whatsnew::{self, Picks};
use bindle_run::whatsnew_state;
use serde_json::{Value, json};

use super::Wizard;

pub struct WhatsNewState {
    pub wn: WhatsNew,
    /// The saved selection planned with this Bindle.
    pub full: Plan,
    pub picks: Picks,
    /// The Review and Apply pages show the picks, not the wizard's selection.
    pub active: bool,
}

impl Wizard {
    /// Look (None when Bindle never applied here, or nothing is new).
    /// `first_run`: only when this version isn't done with yet.
    pub fn load_whats_new(&mut self, root: &Path, first_run: bool) {
        self.whats_new = match whatsnew::look(&self.catalog, &self.facts, root, false) {
            Ok(Some((wn, full)))
                if !wn.is_empty() && (!first_run || whatsnew_state::load().should_open(&wn, whatsnew::VERSION)) =>
            {
                let picks = whatsnew::default_picks(&wn);
                Some(WhatsNewState { wn, full, picks, active: false })
            }
            _ => None,
        };
    }

    /// A made-up What's new for the screenshots.
    pub fn sample_whats_new(&mut self) {
        let item = |id: &str, name: &str, kind, why: &str, ticked, changes: Vec<Change>| Item {
            id: id.into(),
            name: name.into(),
            kind,
            why: why.into(),
            ticked,
            changes,
        };
        let c = |t: &str, d: &[&str]| Change { diff: d.iter().map(|s| s.to_string()).collect(), ..Change::note(t) };
        let k = |t: &str, d: &[&str], key: &str| Change { key: Some(key.into()), ..c(t, d) };
        let wn = WhatsNew {
            version: "0.1.9".into(),
            since: Some("0.1.8".into()),
            recorded: true,
            items: vec![
                item(
                    "spellcheck",
                    "Spell checking",
                    Kind::New,
                    "part of your selection, not set up yet",
                    true,
                    vec![k("+ installs hunspell-en_us", &[], "package:hunspell-en_us")],
                ),
                item(
                    "printing",
                    "Printing (CUPS)",
                    Kind::Updated,
                    "changed since it was set up",
                    true,
                    vec![
                        k("+ installs gutenprint", &[], "package:gutenprint"),
                        k("+ installs foomatic-db", &[], "package:foomatic-db"),
                    ],
                ),
                item(
                    "zram",
                    "zram swap",
                    Kind::Updated,
                    "changed since it was set up",
                    true,
                    vec![k(
                        "~ changes /etc/systemd/zram-generator.conf",
                        &[
                            "  # Managed by Bindle",
                            "  [zram0]",
                            "- zram-size = ram / 2",
                            "+ zram-size = min(ram / 2, 8192)",
                            "  compression-algorithm = zstd",
                        ],
                        "file:/etc/systemd/zram-generator.conf",
                    )],
                ),
                item(
                    "earlyoom",
                    "Early OOM killer (earlyoom)",
                    Kind::Retired,
                    "replaced by Out-of-memory killer (systemd-oomd)",
                    false,
                    vec![
                        c("− turns off earlyoom.service", &[]),
                        c("packages stay installed: pkexec pacman -Rns earlyoom", &[]),
                    ],
                ),
                item(
                    "bash-completion",
                    "Tab completion",
                    Kind::Optional,
                    "Tab completion for commands and their options in bash.",
                    false,
                    vec![],
                ),
            ],
        };
        let mut picks = whatsnew::default_picks(&wn);
        picks.toggle_change("printing", "package:foomatic-db");
        self.whats_new = Some(WhatsNewState { wn, full: Plan::default(), picks, active: false });
    }

    pub fn whats_new_json(&self) -> Value {
        let Some(w) = &self.whats_new else { return json!({ "state": "none" }) };
        let items: Vec<Value> =
            w.wn.items
                .iter()
                .map(|i| {
                    let changes: Vec<Value> = i
                        .changes
                        .iter()
                        .map(|c| {
                            let picked = c.key.as_deref().is_none_or(|k| w.picks.change_picked(&i.id, k));
                            json!({ "text": c.text, "diff": c.diff, "key": c.key, "picked": picked })
                        })
                        .collect();
                    json!({
                        "id": i.id, "name": i.name, "kind": i.kind, "why": i.why,
                        "picked": w.picks.items.contains(&i.id), "changes": changes,
                    })
                })
                .collect();
        json!({
            "state": "ready",
            "active": w.active,
            "version": w.wn.version,
            "since": w.wn.since,
            "recorded": w.wn.recorded,
            "items": items,
            "picked": w.picks.items.len(),
        })
    }

    pub fn toggle_whats_new(&mut self, id: &str) {
        if let Some(w) = &mut self.whats_new
            && w.wn.items.iter().any(|i| i.id == id)
        {
            w.picks.toggle_item(id);
        }
    }

    /// Tick or untick one change of an item (a package, a file, a service).
    pub fn toggle_whats_new_change(&mut self, id: &str, key: &str) {
        if let Some(w) = &mut self.whats_new
            && w.wn.items.iter().any(|i| i.id == id && i.changes.iter().any(|c| c.key.as_deref() == Some(key)))
        {
            w.picks.toggle_change(id, key);
        }
    }

    /// Hide an item for good (until a later Bindle changes it again).
    /// `persist`: remember it (not in screenshot or sandbox runs).
    pub fn hide_whats_new(&mut self, id: &str, persist: bool) -> anyhow::Result<()> {
        let Some(w) = &mut self.whats_new else { return Ok(()) };
        if let Some(item) = w.wn.items.iter().find(|i| i.id == id)
            && persist
        {
            whatsnew_state::hide(item)?;
        }
        w.wn.items.retain(|i| i.id != id);
        w.picks.items.remove(id);
        Ok(())
    }

    /// Open (the Review and Apply pages follow the picks) or close it.
    pub fn set_whats_new_active(&mut self, on: bool) {
        if let Some(w) = &mut self.whats_new {
            w.active = on;
        }
    }

    pub fn whats_new_active(&self) -> bool {
        self.active_whats_new().is_some()
    }

    pub(super) fn active_whats_new(&self) -> Option<&WhatsNewState> {
        self.whats_new.as_ref().filter(|w| w.active)
    }

    /// The plan for the picks.
    pub(super) fn whats_new_plan(&self, w: &WhatsNewState, root: &Path) -> anyhow::Result<Plan> {
        whatsnew::plan_choice(&self.catalog, &self.facts, root, &w.wn, &w.full, &w.picks)
    }

    /// The saved selection after the picks (saved once they are applied).
    pub(super) fn whats_new_selection(&self, w: &WhatsNewState) -> Selection {
        let saved = whatsnew::saved().map(|(s, _)| s).unwrap_or_default();
        whatsnew::selection_after(&saved, &w.wn, &w.picks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_follow_the_ticks_and_the_json() {
        let mut w = Wizard::new(bindle_core::Catalog::arch().unwrap(), bindle_core::Facts::default());
        assert_eq!(w.whats_new_json()["state"], "none");
        w.sample_whats_new();
        let j = w.whats_new_json();
        assert_eq!(j["picked"], 3, "new and updated ticked; retired and optional not");
        let printing = j["items"].as_array().unwrap().iter().find(|i| i["id"] == "printing").unwrap().clone();
        assert_eq!(printing["changes"][1]["picked"], false, "the sample leaves one package out");
        w.toggle_whats_new_change("printing", "package:foomatic-db");
        assert_eq!(w.whats_new_json()["items"][1]["changes"][1]["picked"], true);
        w.hide_whats_new("earlyoom", false).unwrap();
        assert!(w.whats_new_json()["items"].as_array().unwrap().iter().all(|i| i["id"] != "earlyoom"));
        w.toggle_whats_new("bash-completion");
        w.toggle_whats_new("zram");
        w.toggle_whats_new("not-an-item");
        let j = w.whats_new_json();
        assert_eq!(j["picked"], 3);
        let picked =
            |id: &str| j["items"].as_array().unwrap().iter().find(|i| i["id"] == id).unwrap()["picked"].clone();
        assert_eq!(picked("bash-completion"), true);
        assert_eq!(picked("zram"), false);
        assert!(w.active_whats_new().is_none());
        w.set_whats_new_active(true);
        assert!(w.active_whats_new().is_some());
    }
}
