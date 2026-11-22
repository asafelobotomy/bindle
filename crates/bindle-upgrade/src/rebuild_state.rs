//! Packages `checkrebuild` still flags right after Bindle rebuilt them.
//!
//! rebuild-detector flags a package when one of its files links a library
//! that isn't there. For a package that ships prebuilt files (codeql, a
//! driver's userspace with an optional GTK 2 panel…) a rebuild changes
//! nothing: the flag stays, and every run would rebuild it again (and, for a
//! driver, regenerate the boot images). So after the Rebuilds stage Bindle
//! looks again and keeps, in `$XDG_STATE_HOME/bindle/rebuild-stuck.json`, the
//! version of each package the rebuild didn't clear. That version is not
//! rebuilt again; a new version of it, or a package that stops being
//! flagged, drops out of the file.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::system::Rebuild;

/// name → the version a rebuild didn't clear.
pub type Stuck = BTreeMap<String, String>;

/// `$XDG_STATE_HOME/bindle/rebuild-stuck.json` (`~/.local/state/bindle/…`).
pub fn path() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map(|d| d.join("bindle/rebuild-stuck.json"))
}

pub fn load() -> Stuck {
    path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(stuck: &Stuck) {
    let Some(p) = path() else { return };
    if stuck.is_empty() {
        let _ = std::fs::remove_file(&p);
        return;
    }
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(stuck) {
        let _ = std::fs::write(p, json);
    }
}

/// The record after `rebuilt` were rebuilt and `flagged` is what
/// `checkrebuild` says now. Entries for packages no longer flagged, or
/// flagged at another version, go.
pub fn merge(old: &Stuck, rebuilt: &[String], flagged: &[Rebuild]) -> Stuck {
    let now: BTreeMap<&str, &str> = flagged.iter().map(|r| (r.name.as_str(), r.version.as_str())).collect();
    let mut out: Stuck = old
        .iter()
        .filter(|(n, v)| now.get(n.as_str()) == Some(&v.as_str()))
        .map(|(n, v)| (n.clone(), v.clone()))
        .collect();
    for name in rebuilt {
        match now.get(name.as_str()) {
            Some(v) if !v.is_empty() => {
                out.insert(name.clone(), v.to_string());
            }
            _ => {
                out.remove(name);
            }
        }
    }
    out
}

/// After a rebuild ran: look again and keep what it didn't clear. Returns
/// those names.
pub fn settle(rebuilt: &[String]) -> Vec<String> {
    let Some(flagged) = crate::probe::rebuild_now() else { return vec![] };
    let stuck = merge(&load(), rebuilt, &flagged);
    save(&stuck);
    rebuilt.iter().filter(|n| stuck.contains_key(*n)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(name: &str, version: &str) -> Rebuild {
        Rebuild { repo: "foreign".into(), name: name.into(), version: version.into() }
    }

    #[test]
    fn a_rebuild_that_did_not_clear_the_flag_is_kept_until_the_version_changes() {
        let rebuilt = ["codeql".to_string(), "python-foo".to_string()];
        // codeql ships prebuilt files: still flagged; python-foo is fixed.
        let s = merge(&Stuck::new(), &rebuilt, &[r("codeql", "2.27.3-1")]);
        assert_eq!(s, Stuck::from([("codeql".into(), "2.27.3-1".into())]));
        // Next time, nothing rebuilt: still flagged at that version, kept.
        assert_eq!(merge(&s, &[], &[r("codeql", "2.27.3-1")]), s);
        // A new codeql: forgotten, so it is rebuilt again if it is flagged.
        assert!(merge(&s, &[], &[r("codeql", "2.28.0-1")]).is_empty());
        // No longer flagged: forgotten.
        assert!(merge(&s, &[], &[]).is_empty());
    }
}
