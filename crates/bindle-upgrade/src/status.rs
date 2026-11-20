//! What the last check found, kept for uQ's bar: every `bindleup --check`
//! (the terminal's, the daily timer's, uQ Settings' and the update window's)
//! saves it to `$XDG_STATE_HOME/bindle/updates.json`, and a real run removes
//! it, so the next look is a fresh one. `bindle updates --json` reads it
//! without touching the network.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{Stage, StagePlan, Todo};

/// The stages that update something. The others (snapshot, mirrors,
/// config files, orphans…) only come with an update or tidy up after one,
/// so on their own they don't put an icon in the bar.
pub const UPDATES: [Stage; 7] =
    [Stage::Bindle, Stage::Pacman, Stage::Aur, Stage::Releases, Stage::Flatpak, Stage::Firmware, Stage::Rebuild];

/// One stage with something to update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub stage: String,
    pub title: String,
    pub summary: String,
    /// Held back (unread Arch news, battery, free space): why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<String>,
}

/// The last check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    /// When, in seconds since 1970.
    pub checked: u64,
    pub items: Vec<Item>,
}

impl Saved {
    pub fn from_plans(plans: &[StagePlan], checked: u64) -> Saved {
        let items = plans
            .iter()
            .filter(|p| UPDATES.contains(&p.stage))
            .filter_map(|p| {
                let held = match &p.todo {
                    Todo::Run(_) => None,
                    Todo::Hold(why) => Some(why.clone()),
                    _ => return None,
                };
                Some(Item {
                    stage: p.stage.id().into(),
                    title: p.stage.title().into(),
                    summary: p.summary.clone(),
                    held,
                })
            })
            .collect();
        Saved { checked, items }
    }
}

/// `$XDG_STATE_HOME/bindle/updates.json` (`~/.local/state/bindle/updates.json`).
pub fn path() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map(|d| d.join("bindle/updates.json"))
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Save what a check found. Root's checks (`sudo bindleup --check`) aren't
/// saved: the bar belongs to the user. Errors are ignored: the bar just
/// checks again.
pub fn save(plans: &[StagePlan]) {
    if bindle_run::sys::is_root() {
        return;
    }
    if let Some(p) = path() {
        let _ = save_to(&p, &Saved::from_plans(plans, now()));
    }
}

/// After a real run: what was found is out of date.
pub fn clear() {
    if let Some(p) = path() {
        let _ = std::fs::remove_file(p);
    }
}

pub fn load() -> Option<Saved> {
    load_from(&path()?)
}

fn save_to(path: &Path, s: &Saved) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(s)?)?;
    std::fs::rename(&tmp, path)
}

fn load_from(path: &Path) -> Option<Saved> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(stage: Stage, todo: Todo, summary: &str) -> StagePlan {
        StagePlan { stage, todo, summary: summary.into(), details: vec![], ask: None, stops: vec![], packages: vec![] }
    }

    #[test]
    fn only_stages_with_work_are_kept_and_read_back() {
        let plans = [
            plan(Stage::Pacman, Todo::Run(vec![]), "12 packages"),
            plan(Stage::Flatpak, Todo::Nothing, "up to date"),
            plan(Stage::Aur, Todo::Hold("on battery".into()), "2 packages"),
            plan(Stage::Orphans, Todo::Run(vec![]), "3 orphans"),
        ];
        let s = Saved::from_plans(&plans, 42);
        assert_eq!(s.items.len(), 2, "orphans alone are no update");
        assert_eq!(s.items[1].held.as_deref(), Some("on battery"));
        assert_eq!((s.items[0].stage.as_str(), s.items[0].summary.as_str()), ("pacman", "12 packages"));
        let dir = std::env::temp_dir().join(format!("bindle-status-{}", std::process::id()));
        let file = dir.join("bindle/updates.json");
        save_to(&file, &s).unwrap();
        assert_eq!(load_from(&file), Some(s));
        std::fs::write(&file, "not json").unwrap();
        assert_eq!(load_from(&file), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
