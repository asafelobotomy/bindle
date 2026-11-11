//! `bindle status`: the selection Bindle last applied for this user, when, and
//! what of it isn't in place any more. uQ Settings' "Re-run setup" page reads
//! the JSON form (`uq/pages/bindle/`).

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use bindle_core::{Catalog, Facts, Selection, plan};
use bindle_run::selection;
use serde::Serialize;

use crate::print;

#[derive(Debug, Serialize, PartialEq)]
pub struct Status {
    /// The saved selection read, or none when Bindle hasn't applied anything yet.
    pub selection: Option<PathBuf>,
    pub profile: Option<String>,
    /// Modules added to and removed from the profile.
    pub with: Vec<String>,
    pub without: Vec<String>,
    /// When the selection was saved (the last successful apply), in Unix seconds.
    pub last_applied: Option<u64>,
    /// The steps applying the selection again would run, one line each;
    /// empty when everything is in place.
    pub missing: Vec<String>,
    /// The saved selection couldn't be read or planned.
    pub error: Option<String>,
}

/// The user's saved selection, else the one `bindle iso` left for the system.
fn saved(system: &Path) -> Option<PathBuf> {
    selection::default_path().filter(|p| p.is_file()).or_else(|| system.is_file().then(|| system.to_path_buf()))
}

pub fn collect(
    catalog: &Catalog,
    facts: &Facts,
    path: Option<PathBuf>,
    current: &dyn Fn(&str) -> Option<String>,
) -> Status {
    let mut st = Status {
        selection: None,
        profile: None,
        with: vec![],
        without: vec![],
        last_applied: None,
        missing: vec![],
        error: None,
    };
    let Some(path) = path else { return st };
    st.last_applied = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    st.selection = Some(path.clone());
    let sel = match selection::load(&path) {
        Ok(s) => s,
        Err(e) => {
            st.error = Some(format!("{e:#}"));
            return st;
        }
    };
    st.profile.clone_from(&sel.profile);
    st.with.clone_from(&sel.include);
    st.without.clone_from(&sel.exclude);
    // Plan as an interactive apply would, conflicts included.
    let sel = Selection { replace_conflicts: true, ..sel };
    match plan::resolve(catalog, facts, &sel, current) {
        Ok(p) => st.missing = p.steps.iter().map(print::describe).collect(),
        Err(e) => st.error = Some(format!("{e:#}")),
    }
    st
}

pub fn run(catalog: &Catalog, facts: &Facts, json: bool, system: &Path) -> Result<()> {
    let st = collect(catalog, facts, saved(system), &|p| std::fs::read_to_string(p).ok());
    if json {
        println!("{}", serde_json::to_string_pretty(&st)?);
        return Ok(());
    }
    let Some(path) = &st.selection else {
        println!("Bindle hasn't set up anything here yet. Run `bindle` to start.");
        return Ok(());
    };
    println!("Selection: {}", path.display());
    println!("Profile: {}", st.profile.as_deref().unwrap_or("none"));
    if let Some(e) = &st.error {
        println!("It can't be checked: {e}");
    } else if st.missing.is_empty() {
        println!("Everything it set up is still in place.");
    } else {
        println!("Applying it again would:");
        st.missing.iter().for_each(|l| println!("  {l}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_saved_is_not_an_error() {
        let catalog = Catalog::arch().unwrap();
        let st = collect(&catalog, &Facts::default(), None, &|_| None);
        assert_eq!(st.selection, None);
        assert!(st.missing.is_empty() && st.error.is_none());
    }

    #[test]
    fn a_saved_selection_lists_what_is_missing() {
        let dir = std::env::temp_dir().join(format!("bindle-status-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("selection.toml");
        std::fs::write(&path, "profile = \"minimal\"\nwith = [\"fstrim\"]\n").unwrap();
        let catalog = Catalog::arch().unwrap();
        let st = collect(&catalog, &Facts::default(), Some(path.clone()), &|_| None);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(st.profile.as_deref(), Some("minimal"));
        assert_eq!(st.with, ["fstrim"]);
        assert!(st.last_applied.is_some());
        assert!(st.error.is_none(), "{:?}", st.error);
        assert!(!st.missing.is_empty(), "nothing is installed in a blank plan");
    }

    #[test]
    fn a_broken_selection_is_reported() {
        let dir = std::env::temp_dir().join(format!("bindle-status-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("selection.toml");
        std::fs::write(&path, "profile = \"no-such-profile\"\n").unwrap();
        let st = collect(&Catalog::arch().unwrap(), &Facts::default(), Some(path), &|_| None);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(st.error.as_deref().is_some_and(|e| e.contains("no-such-profile")));
    }
}
