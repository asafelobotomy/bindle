//! What the user told What's new, per user (`~/.local/state/bindle/whats-new.json`):
//! the version they skipped (it opens by itself again only for a newer
//! Bindle), and items they hid (each by its content: a later Bindle that
//! changes it again shows it again). "Not now" stores nothing. Also the last
//! version whose release notes were shown (`notes_seen`, `relnotes`).

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
    /// The newest version whose release notes were shown or dismissed, or
    /// that this user installed fresh (never news).
    #[serde(default)]
    pub notes_seen: Option<String>,
}

/// What to do with the release notes of the running version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notes {
    /// Bindle was updated since the notes were last seen: show them once.
    Show,
    /// Bindle hasn't set anything up here (a fresh install): no popup, and
    /// this version counts as seen.
    Baseline,
    /// Already seen (or a downgrade).
    Quiet,
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

impl State {
    /// `set_up`: Bindle has applied something here (a saved selection).
    pub fn notes(&self, version: &str, set_up: bool) -> Notes {
        if self.notes_seen.as_deref().is_some_and(|v| bindle_core::version_cmp(version, v).is_le()) {
            Notes::Quiet
        } else if set_up {
            Notes::Show
        } else {
            Notes::Baseline
        }
    }
}

fn path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("bindle/whats-new.json"))
}

/// For reading: missing or unreadable reads as nothing hidden or skipped.
pub fn load() -> State {
    path().and_then(|p| fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// For changing: a damaged file is kept beside it (`.damaged`) rather than
/// written over with only the one new change.
fn load_for_update() -> Result<State> {
    let Some(p) = path() else { return Ok(State::default()) };
    match fs::read_to_string(&p) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(s) => Ok(s),
            Err(_) => {
                fs::rename(&p, p.with_extension("json.damaged"))?;
                Ok(State::default())
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(e.into()),
    }
}

fn save(s: &State) -> Result<()> {
    let p = path().context("no state directory")?;
    fs::create_dir_all(p.parent().context("state dir")?)?;
    fs::write(&p, serde_json::to_string_pretty(s)?)?;
    Ok(())
}

/// Done with this version (applied, or skipped).
pub fn skip_this_version() -> Result<()> {
    let mut s = load_for_update()?;
    s.skipped = Some(VERSION.into());
    save(&s)
}

/// The release notes of this version are done with (shown, dismissed, or
/// news to nobody: a fresh install, an apply on this version).
pub fn mark_notes_seen() -> Result<()> {
    let mut s = load_for_update()?;
    s.notes_seen = Some(VERSION.into());
    save(&s)
}

pub fn hide(i: &Item) -> Result<()> {
    let mut s = load_for_update()?;
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
    fn release_notes_show_once_after_an_update_never_on_a_fresh_install() {
        let seen = |v: &str| State { notes_seen: Some(v.into()), ..Default::default() };
        assert_eq!(seen("0.1.16").notes("0.1.17", true), Notes::Show, "updated");
        assert_eq!(seen("0.1.17").notes("0.1.17", true), Notes::Quiet, "dismissed stays dismissed");
        assert_eq!(seen("0.2.0").notes("0.1.17", true), Notes::Quiet, "a downgrade");
        assert_eq!(State::default().notes("0.1.17", true), Notes::Show, "set up before notes were tracked");
        assert_eq!(State::default().notes("0.1.17", false), Notes::Baseline, "fresh install");
        assert_eq!(seen("0.1.16").notes("0.1.17", false), Notes::Baseline, "never applied: no popup");
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
