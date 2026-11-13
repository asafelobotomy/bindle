//! What each apply set up, kept apart from the snapshots so it outlives
//! them: an append-only ledger of (apply, module, content) entries. The root
//! helper adds an apply's entries when it completes and drops them when
//! `bindle undo` restores it (`/var/lib/bindle/record.json`); an apply with
//! nothing to do as root, which takes no snapshot, adds its own to the
//! user's ledger (`~/.local/state/bindle/record.json`). "What's new" folds
//! both, plus older snapshots from before the ledger, into one record.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use bindle_core::plan::content::Content;
use bindle_core::whatsnew::Record;
use serde::{Deserialize, Serialize};

use crate::snapshot::Manifest;
use crate::sys::{create_dirs, host, write_atomic};

pub const SYSTEM: &str = "/var/lib/bindle/record.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The apply: its snapshot id, or for a user-only apply its time (same
    /// `YYYYMMDD-HHMMSS` form, so the two sort together).
    pub apply: String,
    pub version: String,
    pub module: String,
    /// None: the apply retired the module (cleaned it up).
    pub content: Option<Content>,
}

impl Ledger {
    /// The entries of one apply: its modules' content and what it retired.
    pub fn add(&mut self, apply: &str, version: &str, record: &BTreeMap<String, Content>, forget: &[String]) {
        self.drop_apply(apply);
        let entry = |module: &String, content| Entry {
            apply: apply.into(),
            version: version.into(),
            module: module.clone(),
            content,
        };
        self.entries.extend(record.iter().map(|(m, c)| entry(m, Some(c.clone()))));
        self.entries.extend(forget.iter().map(|m| entry(m, None)));
    }

    /// `bindle undo` restored this apply: it no longer counts.
    pub fn drop_apply(&mut self, apply: &str) {
        self.entries.retain(|e| e.apply != apply);
    }
}

pub fn user_path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("bindle/record.json"))
}

pub fn load(path: &Path) -> Ledger {
    fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(path: &Path, ledger: &Ledger) -> Result<()> {
    if let Some(dir) = path.parent() {
        create_dirs(dir)?;
    }
    write_atomic(path, serde_json::to_string_pretty(ledger)?.as_bytes(), 0o644)
}

/// The helper, once an apply completed: its snapshot's record goes in.
pub fn on_complete(root: &Path, m: &Manifest) -> Result<()> {
    if m.record.is_empty() && m.forget.is_empty() {
        return Ok(());
    }
    let path = host(root, SYSTEM);
    let mut l = load(&path);
    l.add(&m.id, &m.version, &m.record, &m.forget);
    save(&path, &l)
}

/// The helper, once `bindle undo` restored an apply.
pub fn on_undone(root: &Path, id: &str) -> Result<()> {
    let path = host(root, SYSTEM);
    let mut l = load(&path);
    let before = l.entries.len();
    l.drop_apply(id);
    if l.entries.len() == before { Ok(()) } else { save(&path, &l) }
}

/// Fold the ledgers and the snapshots into one record: entries in apply
/// order (a later one wins; a retirement removes); snapshots count only for
/// applies the ledgers don't have (from before them), and those that named
/// modules without content mark them as set up unrecorded.
pub fn fold(ledgers: &[Ledger], snapshots: &[Manifest]) -> Record {
    let mut entries: Vec<Entry> = ledgers.iter().flat_map(|l| l.entries.iter().cloned()).collect();
    let known: std::collections::BTreeSet<String> = entries.iter().map(|e| e.apply.clone()).collect();
    let mut r = Record::default();
    for m in snapshots.iter().filter(|m| m.complete && !m.undone && !known.contains(&m.id)) {
        if m.record.is_empty() && m.forget.is_empty() {
            r.unrecorded.extend(m.label.split(", ").filter(|s| !s.is_empty()).map(String::from));
        }
        let mut l = Ledger::default();
        l.add(&m.id, &m.version, &m.record, &m.forget);
        entries.extend(l.entries);
    }
    entries.sort_by(|a, b| a.apply.cmp(&b.apply));
    for e in entries {
        match e.content {
            Some(c) => {
                r.modules.insert(e.module.clone(), c);
            }
            None => {
                r.modules.remove(&e.module);
                r.unrecorded.remove(&e.module);
            }
        }
        if !e.version.is_empty() {
            r.version = Some(e.version);
        }
    }
    r.unrecorded.retain(|id| !r.modules.contains_key(id));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(pkgs: &[&str]) -> Content {
        Content { packages: pkgs.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    fn rec(pairs: &[(&str, Content)]) -> BTreeMap<String, Content> {
        pairs.iter().map(|(m, c)| (m.to_string(), c.clone())).collect()
    }

    fn manifest(id: &str, label: &str, record: BTreeMap<String, Content>, undone: bool) -> Manifest {
        let mut m: Manifest = serde_json::from_value(serde_json::json!({
            "id": id, "created": "", "label": label, "user": "u", "files": [], "units": [],
            "packages": [], "aur": [], "complete": true, "undone": undone
        }))
        .unwrap();
        m.record = record;
        m
    }

    #[test]
    fn later_applies_win_and_retirements_remove() {
        let mut sys = Ledger::default();
        sys.add("20261101-100000", "0.1.8", &rec(&[("printing", c(&["cups"]))]), &[]);
        sys.add(
            "20261110-100000",
            "0.1.9",
            &rec(&[("printing", c(&["cups", "gutenprint"]))]),
            &["earlyoom".to_string()],
        );
        let mut user = Ledger::default();
        user.add("20261105-100000", "0.1.8", &rec(&[("rustup", c(&["rustup"]))]), &[]);
        let r = fold(&[sys, user], &[]);
        assert_eq!(r.modules["printing"].packages, ["cups", "gutenprint"]);
        assert!(r.modules.contains_key("rustup"), "user-only applies count too");
        assert_eq!(r.version.as_deref(), Some("0.1.9"));
    }

    #[test]
    fn undo_drops_an_apply_and_the_one_before_counts_again() {
        let mut l = Ledger::default();
        l.add("a1", "0.1.8", &rec(&[("zram", c(&["zram-generator"]))]), &[]);
        l.add("a2", "0.1.9", &rec(&[("zram", c(&["zram-generator", "x"]))]), &[]);
        l.drop_apply("a2");
        assert_eq!(fold(&[l], &[]).modules["zram"].packages, ["zram-generator"]);
    }

    #[test]
    fn snapshots_count_only_for_applies_the_ledger_lacks() {
        let mut l = Ledger::default();
        l.add("20261110-100000", "0.1.9", &rec(&[("printing", c(&["cups", "gutenprint"]))]), &[]);
        let snaps = [
            manifest("20261101-100000", "zram, printing", BTreeMap::new(), false),
            manifest("20261110-100000", "printing", rec(&[("printing", c(&["stale"]))]), false),
            manifest("20261105-100000", "", rec(&[("gone", c(&["x"]))]), true),
        ];
        let r = fold(&[l], &snaps);
        assert_eq!(r.modules["printing"].packages, ["cups", "gutenprint"], "the ledger's copy");
        assert!(!r.modules.contains_key("gone"), "undone");
        assert_eq!(r.unrecorded.iter().collect::<Vec<_>>(), ["zram"]);
    }

    #[test]
    fn the_ledger_survives_on_disk() {
        let dir = crate::sys::test_dir("record-ledger");
        let mut l = Ledger::default();
        l.add("a1", "0.1.9", &rec(&[("zram", c(&["zram-generator"]))]), &["old".to_string()]);
        save(&dir.join("r.json"), &l).unwrap();
        assert_eq!(load(&dir.join("r.json")), l);
        assert_eq!(load(&dir.join("missing.json")), Ledger::default());
    }
}
