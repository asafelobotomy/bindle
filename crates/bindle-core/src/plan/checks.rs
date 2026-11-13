//! Small checks the resolver uses to drop work that is already done.

use std::collections::BTreeSet;

use crate::catalog::{AssetKind, Command, ReleaseAsset};
use crate::detect::Facts;

/// Every `unless` check holds, so the command's effect is already in place.
/// `~/` paths resolve against the user's home (unknown home: not done).
pub(super) fn already_done(c: &Command, facts: &Facts, current_file: &dyn Fn(&str) -> Option<String>) -> bool {
    !c.unless.is_empty()
        && c.unless.iter().all(|u| {
            let path = match u.path.strip_prefix("~/") {
                Some(_) if facts.home.is_empty() => return false,
                Some(rest) => format!("{}/{rest}", facts.home.trim_end_matches('/')),
                None => u.path.clone(),
            };
            u.holds(current_file(&path).as_deref())
        })
}

/// A package by name; an AppImage by its desktop entry naming this asset.
pub(super) fn release_installed(
    r: &ReleaseAsset,
    facts: &Facts,
    current_file: &dyn Fn(&str) -> Option<String>,
) -> bool {
    match r.kind() {
        Some(AssetKind::Package) => facts.installed.contains(&r.name),
        Some(AssetKind::AppImage) if !facts.home.is_empty() => {
            let entry = r.desktop_entry();
            let path = format!("{}/{}", facts.home.trim_end_matches('/'), entry.trim_start_matches("~/"));
            current_file(&path).is_some_and(|c| c.contains(&format!("X-Bindle-Asset={}", r.asset)))
        }
        _ => false,
    }
}

pub(super) fn dedup(items: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    items.into_iter().filter(|i| seen.insert(i.clone())).collect()
}
