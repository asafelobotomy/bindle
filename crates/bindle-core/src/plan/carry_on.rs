//! Carrying on after part of an AUR step failed: the selection without the modules
//! whose packages didn't install, so one package that can't build (a source host that
//! is down, a failing test) doesn't leave the rest of the setup undone.

use std::collections::BTreeSet;

use super::Selection;
use crate::catalog::Catalog;

/// `sel` without the modules of `planned` that install one of `failed` (AUR package
/// names), or that need such a module. Returns it with the modules left out, or None
/// when nothing can be carried on with: no module owns the packages, or one of them
/// installs the desktop itself (the rest of the plan was made for that desktop).
pub fn without_packages(
    catalog: &Catalog,
    sel: &Selection,
    planned: &[String],
    failed: &[String],
) -> Option<(Selection, Vec<String>)> {
    let module = |id: &String| catalog.module(id);
    let mut out: BTreeSet<&String> =
        planned.iter().filter(|id| module(id).is_some_and(|m| m.aur.iter().any(|p| failed.contains(p)))).collect();
    if out.is_empty() {
        return None;
    }
    loop {
        let more: Vec<&String> = planned
            .iter()
            .filter(|id| !out.contains(id) && module(id).is_some_and(|m| m.requires.iter().any(|r| out.contains(r))))
            .collect();
        if more.is_empty() {
            break;
        }
        out.extend(more);
    }
    let desktops = crate::desktops::all().unwrap_or_default();
    if desktops.iter().any(|d| d.module.as_ref().is_some_and(|m| out.contains(m))) {
        return None;
    }
    let mut rest = sel.clone();
    rest.include.retain(|id| !out.contains(id));
    rest.exclude.extend(out.iter().map(|id| id.to_string()));
    Some((rest, out.into_iter().cloned().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel() -> Selection {
        Selection {
            profile: Some("desktop".into()),
            include: ["uq", "kyko", "uqbit", "noti-fi"].map(String::from).to_vec(),
            ..Default::default()
        }
    }

    fn planned() -> Vec<String> {
        ["aur-helper", "bindle", "uq", "kyko", "uqbit", "noti-fi", "oysterav", "zram"].map(String::from).to_vec()
    }

    #[test]
    fn the_modules_of_failed_packages_are_left_out() {
        let c = Catalog::arch().unwrap();
        let failed = ["kyko".to_string(), "chkrootkit".to_string()];
        let (rest, out) = without_packages(&c, &sel(), &planned(), &failed).unwrap();
        assert_eq!(out, ["kyko", "oysterav"]);
        assert_eq!(rest.include, ["uq", "uqbit", "noti-fi"]);
        assert!(rest.exclude.contains(&"kyko".to_string()) && rest.exclude.contains(&"oysterav".to_string()));
        // What is left still resolves: nothing kept needs what was left out.
        let p = super::super::resolve(&c, &crate::detect::Facts::default(), &rest, &|_| None).unwrap();
        assert!(p.modules.contains(&"uq".to_string()) && !p.modules.contains(&"kyko".to_string()));
    }

    #[test]
    fn what_needs_a_left_out_module_goes_with_it() {
        let c = Catalog::arch().unwrap();
        let planned = ["scdr", "scdr-sched-ext", "kyko"].map(String::from).to_vec();
        let (rest, out) = without_packages(&c, &sel(), &planned, &["scdr".into()]).unwrap();
        assert_eq!(out, ["scdr", "scdr-sched-ext"]);
        assert!(rest.include.contains(&"kyko".to_string()));
        // paru-bin is the AUR helper's own step, not in a module's `aur` list.
        let (_, out) = without_packages(&c, &sel(), &self::planned(), &["paru-bin".into(), "bindle".into()]).unwrap();
        assert_eq!(out, ["bindle"]);
    }

    #[test]
    fn a_desktop_that_did_not_install_stops_the_setup() {
        let c = Catalog::arch().unwrap();
        assert!(without_packages(&c, &sel(), &planned(), &["uq".into()]).is_none());
        assert!(without_packages(&c, &sel(), &planned(), &["not-in-the-catalog".into()]).is_none());
    }
}
