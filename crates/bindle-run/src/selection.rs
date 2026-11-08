//! Selections saved as TOML so a setup can be replayed on another machine:
//! `bindle apply --from selection.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bindle_core::Selection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionFile {
    profile: Option<String>,
    #[serde(default)]
    with: Vec<String>,
    #[serde(default)]
    without: Vec<String>,
}

pub fn default_path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|c| c.join("bindle/selection.toml"))
}

pub fn load(path: &Path) -> Result<Selection> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let f: SelectionFile = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(Selection { profile: f.profile, include: f.with, exclude: f.without, ..Default::default() })
}

/// The selection as a commented TOML file.
pub fn render(sel: &Selection) -> Result<String> {
    let f = SelectionFile { profile: sel.profile.clone(), with: sel.include.clone(), without: sel.exclude.clone() };
    let body = toml::to_string(&f)?;
    Ok(format!("# Bindle selection. Replay with: bindle apply --from <this file>\n{body}"))
}

pub fn save(sel: &Selection, path: &Path) -> Result<()> {
    crate::sys::write_atomic(path, render(sel)?.as_bytes(), 0o644)
}

/// Layer CLI arguments over a file selection.
pub fn merge(base: Selection, over: Selection) -> Selection {
    let mut s = base;
    if over.profile.is_some() {
        s.profile = over.profile;
    }
    s.include.extend(over.include);
    s.exclude.extend(over.exclude);
    s.include.retain(|m| !s.exclude.contains(m));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_merges() {
        let dir = std::env::temp_dir().join(format!("bindle-sel-{}", std::process::id()));
        let path = dir.join("s.toml");
        let sel = Selection {
            profile: Some("gaming".into()),
            include: vec!["vscode".into()],
            exclude: vec!["ufw".into()],
            ..Default::default()
        };
        save(&sel, &path).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.profile.as_deref(), Some("gaming"));
        let merged = merge(back, Selection { exclude: vec!["vscode".into()], ..Default::default() });
        assert!(merged.include.is_empty() && merged.exclude.len() == 2);
        let _ = fs::remove_dir_all(dir);
    }
}
