//! "What's new" after upgrading Bindle: the saved selection planned with
//! this Bindle's catalog, compared module by module with what earlier
//! applies recorded (`plan::content::Content`). No hand-written migrations:
//!
//! - **New**: modules the selection now brings that were never applied
//!   (ticked), and optional modules `changes.toml` lists for this version
//!   (unticked).
//! - **Updated**: applied modules whose content changed (a package added, a
//!   file's template, a release pin), with the exact before → after (ticked).
//! - **Retired**: applied modules that left the catalog or the selection,
//!   with what they left behind (clean-up unticked).
//!
//! Pure: the record, the plan and what is on the system come in as data.
//! Installs from before records existed are compared with what is installed
//! (`recorded: false`), and say so.

pub mod changes;
pub mod linediff;
pub mod pick;

use std::collections::BTreeMap;

use serde::Serialize;

use crate::catalog::Catalog;
use crate::detect::Facts;
use crate::plan::Plan;
use crate::plan::content::Content;
pub use changes::Changes;

/// What earlier applies recorded, merged (a later apply's content wins).
#[derive(Debug, Clone, Default)]
pub struct Record {
    /// The newest Bindle version that applied anything here.
    pub version: Option<String>,
    pub modules: BTreeMap<String, Content>,
    /// Modules applies named before they recorded content: compared with
    /// what is installed.
    pub unrecorded: std::collections::BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    New,
    Optional,
    Updated,
    /// Left Bindle, or replaced by another module.
    Retired,
    /// Taken out of the saved selection by the user; never cleaned up.
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub text: String,
    /// For a changed file: its line diff (`- old`, `+ new`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diff: Vec<String>,
    /// Set when this change can be left out on its own (`package:…`,
    /// `aur:…`, `file:…`, `unit:…`; see `pick`): the rest follows the module.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl Change {
    pub fn note(text: impl Into<String>) -> Self {
        Change { text: text.into(), diff: vec![], key: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub why: String,
    /// Chosen unless the user unticks it.
    pub ticked: bool,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WhatsNew {
    /// The Bindle comparing.
    pub version: String,
    /// The Bindle that last applied here, when recorded.
    pub since: Option<String>,
    /// Earlier applies recorded what they did; else the comparison is with
    /// what is installed.
    pub recorded: bool,
    pub items: Vec<Item>,
}

impl WhatsNew {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// What is on the system now.
pub struct Now<'a> {
    pub facts: &'a Facts,
    pub current_file: &'a dyn Fn(&str) -> Option<String>,
    /// Modules the user took out of their saved selection (`exclude`).
    pub excluded: &'a std::collections::BTreeSet<String>,
}

impl Now<'_> {
    fn installed(&self, p: &str) -> bool {
        self.facts.has_package(p)
    }

    /// `c` isn't fully in place: a package, file, unit or group missing.
    fn pending(&self, c: &Content) -> bool {
        c.packages.iter().chain(&c.aur).any(|p| !self.installed(p))
            || c.files.iter().any(|(p, text)| (self.current_file)(p).as_deref() != Some(text))
            || c.units.iter().any(|u| !self.facts.enabled_units.contains(u))
            || c.groups.iter().any(|g| !self.facts.user_groups.contains(g))
    }

    /// Something of `c` is here already (an install made before records).
    fn partly(&self, c: &Content) -> bool {
        c.packages.iter().chain(&c.aur).any(|p| self.installed(p))
            || c.files.keys().any(|p| (self.current_file)(p).is_some())
    }

    /// What the system has, as a content to compare with (no record).
    fn as_found(&self, c: &Content) -> Content {
        let keep = |v: &[String], f: &dyn Fn(&str) -> bool| v.iter().filter(|x| f(x)).cloned().collect();
        Content {
            packages: keep(&c.packages, &|p| self.installed(p)),
            aur: keep(&c.aur, &|p| self.installed(p)),
            files: c.files.keys().filter_map(|p| Some((p.clone(), (self.current_file)(p)?))).collect(),
            units: keep(&c.units, &|u| self.facts.enabled_units.contains(u)),
            groups: keep(&c.groups, &|g| self.facts.user_groups.contains(g)),
            ..c.clone()
        }
    }
}

/// `kind`: what an addition's key starts with, when it can be left out alone.
fn list_changes(out: &mut Vec<Change>, old: &[String], new: &[String], add: &str, drop: &str, kind: Option<&str>) {
    for x in new.iter().filter(|x| !old.contains(x)) {
        out.push(Change { key: kind.map(|k| format!("{k}:{x}")), ..Change::note(format!("+ {add} {x}")) });
    }
    for x in old.iter().filter(|x| !new.contains(x)) {
        out.push(Change::note(format!("− {drop} {x}")));
    }
}

/// Every difference between what was applied (`old`) and what the module
/// does now (`new`).
pub fn changes(old: &Content, new: &Content) -> Vec<Change> {
    let mut out = vec![];
    let kept = "no longer installs (it stays installed)";
    list_changes(&mut out, &old.packages, &new.packages, "installs", kept, Some("package"));
    list_changes(&mut out, &old.aur, &new.aur, "installs from the AUR", kept, Some("aur"));
    list_changes(&mut out, &old.releases, &new.releases, "installs the release", "replaces the release", None);
    for (path, text) in &new.files {
        match old.files.get(path) {
            None => out.push(Change {
                text: format!("+ writes {path}"),
                diff: linediff::diff("", text),
                key: Some(format!("file:{path}")),
            }),
            Some(was) if was != text => out.push(Change {
                text: format!("~ changes {path}"),
                diff: linediff::diff(was, text),
                key: Some(format!("file:{path}")),
            }),
            Some(_) => {}
        }
    }
    for path in old.files.keys().filter(|p| !new.files.contains_key(*p)) {
        out.push(Change::note(format!("− no longer writes {path} (it is left in place)")));
    }
    list_changes(&mut out, &old.units, &new.units, "enables", "no longer enables", Some("unit"));
    let groups = ("adds you to the group", "no longer adds you to the group");
    list_changes(&mut out, &old.groups, &new.groups, groups.0, groups.1, None);
    list_changes(&mut out, &old.commands, &new.commands, "runs", "no longer runs", None);
    list_changes(&mut out, &old.pacman, &new.pacman, "sets in pacman.conf", "no longer sets in pacman.conf", None);
    out
}

/// Compare. `plan`: the saved selection planned with this Bindle (its
/// `contents` and `modules`); `version`: this Bindle's.
pub fn diff(record: &Record, plan: &Plan, catalog: &Catalog, table: &Changes, now: &Now, version: &str) -> WhatsNew {
    let recorded = !record.modules.is_empty();
    let applied: BTreeMap<String, Content> =
        record.modules.iter().map(|(id, c)| (table.current_id(id).to_string(), c.clone())).collect();
    let name = |id: &str| catalog.module(id).map_or(id.to_string(), |m| m.name.clone());
    let added_why = |id: &str| {
        table.added.iter().find(|a| a.modules.iter().any(|m| m == id)).map(|a| format!("new in Bindle {}", a.version))
    };
    // Applied before records: named by an old apply, or (with no trace at
    // all) partly installed.
    let applied_unrecorded = |id: &str, c: &Content| {
        record.unrecorded.contains(id) || (!recorded && record.unrecorded.is_empty() && now.partly(c))
    };
    let mut items = vec![];
    for id in &plan.modules {
        let Some(c) = plan.contents.get(id) else { continue };
        let item =
            |kind, why: String, ticked, changes| Item { id: id.clone(), name: name(id), kind, why, ticked, changes };
        match applied.get(id) {
            Some(old) if old != c => {
                let ch = changes(old, c);
                if !ch.is_empty() {
                    items.push(item(Kind::Updated, "changed since it was set up".into(), true, ch));
                }
            }
            Some(_) => {}
            None if !now.pending(c) => {}
            None if applied_unrecorded(id, c) => {
                let why = "set up before Bindle kept a record: compared with what is installed".to_string();
                items.push(item(Kind::Updated, why, true, changes(&now.as_found(c), c)));
            }
            None => {
                let why = added_why(id).unwrap_or_else(|| "in your setup, not on this computer yet".into());
                items.push(item(Kind::New, why, true, changes(&Content::default(), c)));
            }
        }
    }
    // Optional modules new in versions after the last apply, not selected.
    let newer = |v: &str| record.version.as_deref().is_none_or(|since| crate::version_cmp(v, since).is_gt());
    for a in table.added.iter().filter(|a| recorded && newer(&a.version)) {
        for id in a.modules.iter().filter(|m| !plan.modules.contains(m) && !applied.contains_key(*m)) {
            let Some(m) = catalog.module(id) else { continue };
            if now.facts.matches(&m.when).is_err() || items.iter().any(|i| &i.id == id) {
                continue;
            }
            items.push(Item {
                id: id.clone(),
                name: m.name.clone(),
                kind: Kind::Optional,
                why: m.description.clone(),
                ticked: false,
                changes: vec![],
            });
        }
    }
    // Applied, but no longer in the catalog or the selection.
    for (id, old) in &applied {
        if plan.modules.contains(id) {
            continue;
        }
        let retired = table.retired.iter().find(|r| &r.id == id);
        let rival = plan.modules.iter().find(|m| {
            catalog.module(m).is_some_and(|x| x.conflicts.contains(id))
                || catalog.module(id).is_some_and(|x| x.conflicts.contains(m))
        });
        let (kind, why) = match (retired, rival, catalog.module(id)) {
            (Some(r), _, _) => (Kind::Retired, r.why.clone()),
            (None, Some(by), _) => (Kind::Retired, format!("replaced by {}", name(by))),
            (None, None, None) => (Kind::Retired, "no longer part of Bindle".into()),
            (None, None, Some(_)) if now.excluded.contains(id) => {
                (Kind::Removed, "you took it out of your setup; what it set up is still here".into())
            }
            (None, None, Some(m)) if now.facts.matches(&m.when).is_err() => {
                (Kind::Retired, "doesn't fit this computer any more".into())
            }
            (None, None, Some(_)) => (Kind::Retired, "no longer in your profile".into()),
        };
        items.push(Item { id: id.clone(), name: name(id), kind, why, ticked: false, changes: cleanup(old) });
    }
    WhatsNew { version: version.into(), since: record.version.clone(), recorded, items }
}

/// What a retired module's clean-up does: its files go (only ones still as
/// Bindle wrote them), its units are turned off; packages stay.
pub fn cleanup(old: &Content) -> Vec<Change> {
    let mut out: Vec<Change> =
        old.files.keys().map(|p| Change::note(format!("− removes {p} (if Bindle wrote it)"))).collect();
    out.extend(old.units.iter().map(|u| Change::note(format!("− turns off {u}"))));
    let pkgs: Vec<&String> = old.packages.iter().chain(&old.aur).collect();
    if !pkgs.is_empty() {
        let list = pkgs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ");
        out.push(Change::note(format!("packages stay installed: pkexec pacman -Rns {list}")));
    }
    out
}

#[cfg(test)]
#[path = "whatsnew/tests.rs"]
mod tests;
