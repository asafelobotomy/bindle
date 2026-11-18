//! Package groups (`pacman -Sgg`), cached between runs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

use super::system::stdout;

const SYNC_DIR: &str = "/var/lib/pacman/sync";

/// Sizes and mtimes of the sync databases: when they change, so do the groups.
fn sync_fingerprint() -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let mut parts: Vec<String> = fs::read_dir(SYNC_DIR)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some(format!("{}:{}:{}", e.file_name().to_string_lossy(), m.len(), m.mtime()))
        })
        .collect();
    parts.sort();
    (!parts.is_empty()).then(|| parts.join(","))
}

fn cache_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".cache")))?;
    Some(base.join("bindle/package-groups"))
}

fn parse_groups(text: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in text.lines() {
        if let Some((group, pkg)) = line.split_once(' ') {
            out.entry(group.to_string()).or_default().insert(pkg.to_string());
        }
    }
    out
}

/// `pacman -Sgg` parses every sync database (the bulk of detection's time), so
/// the answer is kept in the user's cache until a database changes.
pub(super) fn load() -> BTreeMap<String, BTreeSet<String>> {
    let (fingerprint, path) = (sync_fingerprint(), cache_path());
    if let (Some(fp), Some(path)) = (&fingerprint, &path)
        && let Ok(cached) = fs::read_to_string(path)
        && let Some((head, body)) = cached.split_once('\n')
        && head == fp
    {
        return parse_groups(body);
    }
    let text = stdout(Command::new("pacman").arg("-Sgg")).unwrap_or_default();
    if let (Some(fp), Some(path)) = (&fingerprint, &path)
        && !text.is_empty()
    {
        // Best effort: a read-only cache dir just means the next run asks pacman again.
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, format!("{fp}\n{text}"));
    }
    parse_groups(&text)
}

#[cfg(test)]
mod tests {
    use crate::Facts;

    #[test]
    fn groups_count_as_installed_when_complete() {
        let mut f = Facts::default();
        f.groups.set([("deepin-extra".to_string(), ["a", "b"].map(String::from).into())].into()).unwrap();
        f.installed.insert("a".into());
        assert!(!f.has_package("deepin-extra"));
        f.installed.insert("b".into());
        assert!(f.has_package("deepin-extra") && f.has_package("a"));
    }
}
