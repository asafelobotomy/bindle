//! "What's new" on this system: the record earlier applies left in their
//! snapshots, the saved selection planned with this Bindle, and the plan for
//! what the user picks (only that: an exact selection), plus the
//! last-seen version that decides when it opens by itself.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use bindle_core::whatsnew::{self, Changes, Kind, Now, Record, WhatsNew, pick};
use bindle_core::{Catalog, Facts, Plan, Selection, Step, plan};

use crate::{selection, snapshot};

/// Where `bindle iso` leaves the selection for the whole system.
pub const SYSTEM_SELECTION: &str = "/etc/bindle/selection.toml";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What earlier applies set up here: the system's and the user's ledgers
/// (`record`), and snapshots from before the ledger.
pub fn record(root: &Path) -> Record {
    let mut ledgers = vec![crate::record::load(&crate::sys::host(root, crate::record::SYSTEM))];
    if root == Path::new("/")
        && let Some(p) = crate::record::user_path()
    {
        ledgers.push(crate::record::load(&p));
    }
    crate::record::fold(&ledgers, &snapshot::list(root))
}

/// The user's saved selection, else the system's (from the installer).
pub fn saved() -> Option<(Selection, PathBuf)> {
    let user = selection::default_path().filter(|p| p.is_file());
    let path = user.unwrap_or_else(|| SYSTEM_SELECTION.into());
    selection::load(&path).ok().map(|s| (s, path))
}

fn current(root: &Path) -> impl Fn(&str) -> Option<String> + '_ {
    move |p: &str| fs::read_to_string(crate::sys::host(root, p)).ok()
}

/// Compare this Bindle with what earlier applies did here, without the
/// items the user hid unless `all`. None: nothing saved to compare (Bindle
/// never applied here).
pub fn look(catalog: &Catalog, facts: &Facts, root: &Path, all: bool) -> Result<Option<(WhatsNew, Plan)>> {
    let Some((sel, _)) = saved() else { return Ok(None) };
    let table = Changes::arch(catalog)?;
    let cur = current(root);
    let excluded: BTreeSet<String> = sel.exclude.iter().cloned().collect();
    let full = plan::resolve(catalog, facts, &Selection { replace_conflicts: true, ..sel }, &cur)?;
    let now = Now { facts, current_file: &cur, excluded: &excluded };
    let wn = whatsnew::diff(&record(root), &full, catalog, &table, &now, VERSION);
    let wn = if all { wn } else { crate::whatsnew_state::load().visible(wn) };
    Ok(Some((wn, full)))
}

/// What the user picked: whole items, and single changes left out of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Picks {
    pub items: BTreeSet<String>,
    /// (item id, change key) left out of a picked item.
    pub left_out: BTreeSet<(String, String)>,
}

impl Picks {
    pub fn toggle_item(&mut self, id: &str) {
        if !self.items.remove(id) {
            self.items.insert(id.to_string());
        }
    }

    pub fn toggle_change(&mut self, id: &str, key: &str) {
        let k = (id.to_string(), key.to_string());
        if !self.left_out.remove(&k) {
            self.left_out.insert(k);
        }
    }

    /// The change keys left out of item `id`.
    pub fn left_out_of(&self, id: &str) -> BTreeSet<String> {
        self.left_out.iter().filter(|(i, _)| i == id).map(|(_, k)| k.clone()).collect()
    }

    pub fn change_picked(&self, id: &str, key: &str) -> bool {
        !self.left_out.contains(&(id.to_string(), key.to_string()))
    }
}

/// The plan for exactly the picked items: new and updated modules (without
/// the single changes left out of them), plus the clean-up of picked
/// retired ones (their files, if Bindle wrote them, and units no other
/// module uses; packages stay).
pub fn plan_choice(
    catalog: &Catalog,
    facts: &Facts,
    root: &Path,
    wn: &WhatsNew,
    full: &Plan,
    picks: &Picks,
) -> Result<Plan> {
    let picked = &picks.items;
    let chosen = |k: Kind| wn.items.iter().filter(move |i| i.kind == k && picked.contains(&i.id));
    let include: Vec<String> =
        chosen(Kind::New).chain(chosen(Kind::Optional)).chain(chosen(Kind::Updated)).map(|i| i.id.clone()).collect();
    let mut p = if include.is_empty() {
        Plan::default()
    } else {
        let sel = Selection { include, exact: true, replace_conflicts: true, ..Default::default() };
        plan::resolve(catalog, facts, &sel, &current(root))?
    };
    let rec = record(root);
    // Changes left out: their steps go, and the record keeps the old value.
    let left: BTreeSet<String> =
        picks.left_out.iter().filter(|(id, _)| picked.contains(id)).map(|(_, k)| k.clone()).collect();
    if !left.is_empty() {
        p.steps = pick::filter_steps(std::mem::take(&mut p.steps), &left);
        for id in picked {
            let keys = picks.left_out_of(id);
            if let Some(c) = p.contents.get(id).cloned().filter(|_| !keys.is_empty()) {
                p.contents.insert(id.clone(), pick::revert(&c, rec.modules.get(id), &keys));
            }
        }
    }
    let used: BTreeSet<&String> = full.contents.values().flat_map(|c| &c.units).collect();
    for item in chosen(Kind::Retired).chain(chosen(Kind::Removed)) {
        let Some(old) = rec.modules.get(&item.id) else { continue };
        if !old.files.is_empty() {
            p.steps.push(Step::RemoveFiles { paths: old.files.keys().cloned().collect() });
        }
        let units: Vec<String> = old.units.iter().filter(|u| !used.contains(u)).cloned().collect();
        if !units.is_empty() {
            p.steps.push(Step::DisableUnits { units });
        }
        p.forget.push(item.id.clone());
    }
    Ok(p)
}

/// The saved selection after the picks: optional modules joined it, cleaned
/// up modules left it.
pub fn selection_after(sel: &Selection, wn: &WhatsNew, picks: &Picks) -> Selection {
    let picked = &picks.items;
    let mut s = sel.clone();
    for i in wn.items.iter().filter(|i| picked.contains(&i.id)) {
        match i.kind {
            Kind::Optional if !s.include.contains(&i.id) => s.include.push(i.id.clone()),
            Kind::Retired | Kind::Removed => {
                s.include.retain(|m| m != &i.id);
            }
            _ => {}
        }
    }
    s
}

/// The items ticked by default, with every change in them.
pub fn default_picks(wn: &WhatsNew) -> Picks {
    Picks { items: wn.items.iter().filter(|i| i.ticked).map(|i| i.id.clone()).collect(), left_out: BTreeSet::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::SnapshotPlan;
    use bindle_core::plan::content::Content;

    fn apply(root: &Path, label: &str, record: &[(&str, &[&str])], forget: &[&str], version: &str) {
        let plan = SnapshotPlan {
            label: label.into(),
            version: version.into(),
            record: record
                .iter()
                .map(|(id, pkgs)| {
                    (
                        id.to_string(),
                        Content { packages: pkgs.iter().map(|s| s.to_string()).collect(), ..Default::default() },
                    )
                })
                .collect(),
            forget: forget.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        let m = snapshot::create(root, &plan).unwrap();
        snapshot::mark_complete(root, &m.id).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100)); // ids are per second
    }

    #[test]
    fn records_merge_newest_last_and_forget_retired() {
        let root = crate::sys::test_dir("whatsnew-record");
        apply(&root, "printing, zram", &[], &[], "");
        apply(&root, "printing, earlyoom", &[("printing", &["cups"]), ("earlyoom", &["earlyoom"])], &[], "0.1.9");
        apply(&root, "", &[("printing", &["cups", "gutenprint"])], &["earlyoom"], "0.2.0");
        let r = record(&root);
        assert_eq!(r.version.as_deref(), Some("0.2.0"));
        assert_eq!(r.modules["printing"].packages, ["cups", "gutenprint"]);
        assert!(!r.modules.contains_key("earlyoom"), "forgotten by its clean-up");
        assert_eq!(r.unrecorded.iter().collect::<Vec<_>>(), ["zram"], "applied before records");
    }

    #[test]
    fn picks_join_or_leave_the_saved_selection() {
        let item = |id: &str, kind| whatsnew::Item {
            id: id.into(),
            name: id.into(),
            kind,
            why: String::new(),
            ticked: false,
            changes: vec![],
        };
        let wn = WhatsNew {
            items: vec![
                item("bash-completion", Kind::Optional),
                item("earlyoom", Kind::Retired),
                item("printing", Kind::Updated),
            ],
            ..Default::default()
        };
        let sel = Selection { include: vec!["earlyoom".into()], ..Default::default() };
        let picks =
            Picks { items: ["bash-completion", "earlyoom", "printing"].map(String::from).into(), ..Default::default() };
        let after = selection_after(&sel, &wn, &picks);
        assert_eq!(after.include, ["bash-completion"]);
    }
}
