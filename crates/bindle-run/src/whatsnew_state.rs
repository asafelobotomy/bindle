//! What the user told What's new, per user (`~/.local/state/bindle/whats-new.json`):
//! the version they skipped (it opens by itself again only for a newer
//! Bindle), and items they hid (each by its content: a later Bindle that
//! changes it again shows it again). "Not now" stores nothing.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use bindle_core::whatsnew::{Item, WhatsNew};
use serde::{Deserialize, Serialize};

use crate::whatsnew::VERSION;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// Done with this version: applied, or "Skip this version".
    #[serde(default)]
    pub skipped: Option<String>,
    /// `item_key`s of hidden items.
    #[serde(default)]
    pub hidden: BTreeSet<String>,
}

/// An item and what it changes (FNV-1a of its changes): hiding it hides
/// this version of it only.
pub fn item_key(i: &Item) -> String {
    let text = serde_json::to_string(&i.changes).unwrap_or_default();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in i.id.bytes().chain(text.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{}@{h:016x}", i.id)
}

impl State {
    /// `wn` without the hidden items.
    pub fn visible(&self, mut wn: WhatsNew) -> WhatsNew {
        wn.items.retain(|i| !self.hidden.contains(&item_key(i)));
        wn
    }

    /// Open by itself: something to show, and this version not done with.
    pub fn should_open(&self, wn: &WhatsNew, version: &str) -> bool {
        !wn.is_empty() && self.skipped.as_deref().is_none_or(|v| bindle_core::version_cmp(version, v).is_gt())
    }
}

fn path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("bindle/whats-new.json"))
}

pub fn load() -> State {
    path().and_then(|p| fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(s: &State) -> Result<()> {
    let p = path().context("no state directory")?;
    fs::create_dir_all(p.parent().context("state dir")?)?;
    fs::write(&p, serde_json::to_string_pretty(s)?)?;
    Ok(())
}

/// Done with this version (applied, or skipped).
pub fn skip_this_version() -> Result<()> {
    let mut s = load();
    s.skipped = Some(VERSION.into());
    save(&s)
}

pub fn hide(i: &Item) -> Result<()> {
    let mut s = load();
    s.hidden.insert(item_key(i));
    save(&s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bindle_core::whatsnew::{Change, Kind};

    fn item(id: &str, change: &str) -> Item {
        Item {
            id: id.into(),
            name: id.into(),
            kind: Kind::Updated,
            why: String::new(),
            ticked: true,
            changes: vec![Change::note(change)],
        }
    }

    #[test]
    fn hidden_items_come_back_when_they_change_again() {
        let s = State { hidden: [item_key(&item("printing", "+ installs gutenprint"))].into(), ..Default::default() };
        let wn = WhatsNew {
            items: vec![item("printing", "+ installs gutenprint"), item("zram", "x")],
            ..Default::default()
        };
        assert_eq!(s.visible(wn).items.len(), 1);
        let changed = WhatsNew { items: vec![item("printing", "+ installs foomatic-db")], ..Default::default() };
        assert_eq!(s.visible(changed).items.len(), 1);
    }

    #[test]
    fn it_opens_by_itself_until_the_version_is_done() {
        let wn = WhatsNew { items: vec![item("zram", "x")], ..Default::default() };
        assert!(State::default().should_open(&wn, "0.1.9"), "Not now stores nothing");
        let skipped = State { skipped: Some("0.1.9".into()), ..Default::default() };
        assert!(!skipped.should_open(&wn, "0.1.9"));
        assert!(skipped.should_open(&wn, "0.2.0"), "a newer Bindle");
        assert!(!State::default().should_open(&WhatsNew::default(), "0.1.9"), "nothing to show");
    }
}
