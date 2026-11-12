//! Retiring what a conflicting module left behind.
//!
//! Choosing one of a `conflicts` pair after the other was applied (oomd, then
//! earlyoom) must not leave both running: the plan disables the rival's units
//! and removes the files Bindle wrote for it. Only untouched Bindle files go;
//! one edited since is kept and reported.

use std::collections::BTreeSet;

use super::Step;
use crate::catalog::Catalog;
use crate::detect::Facts;

/// First line of every file Bindle writes.
pub const MARKER: &str = "# Managed by Bindle";

#[derive(Debug, Default)]
pub(super) struct Retired {
    pub steps: Vec<Step>,
    pub notes: Vec<(String, String)>,
}

/// `chosen` is the final module list; `wanted_units` and `wanted_files` are
/// everything those modules enable and write, which is never retired.
pub(super) fn rivals(
    catalog: &Catalog,
    chosen: &[String],
    facts: &Facts,
    current_file: &dyn Fn(&str) -> Option<String>,
    wanted_units: &BTreeSet<&str>,
    wanted_files: &BTreeSet<&str>,
) -> Retired {
    let mut out = Retired::default();
    let (mut units, mut paths) = (Vec::new(), Vec::new());
    let mut seen = BTreeSet::new();
    for id in chosen {
        let rivals = &catalog.module(id).expect("validated").conflicts;
        for rival in rivals.iter().filter(|r| !chosen.contains(r)) {
            if !seen.insert(rival.as_str()) {
                continue;
            }
            let r = catalog.module(rival).expect("validated");
            for u in r.enable.iter().filter(|u| facts.enabled_units.contains(*u) && !wanted_units.contains(u.as_str()))
            {
                if !units.contains(u) {
                    units.push(u.clone());
                }
            }
            for f in r.files.iter().filter(|f| !wanted_files.contains(f.path.as_str())) {
                match current_file(&f.path) {
                    Some(now) if now == f.content => paths.push(f.path.clone()),
                    Some(now) if now.starts_with(MARKER) => out.notes.push((
                        id.clone(),
                        format!("{} (from {rival}) was edited since Bindle wrote it; left in place.", f.path),
                    )),
                    _ => {}
                }
            }
        }
    }
    if !units.is_empty() {
        out.steps.push(Step::DisableUnits { units });
    }
    if !paths.is_empty() {
        out.steps.push(Step::RemoveFiles { paths });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{Plan, Selection, resolve};
    use super::*;

    fn facts() -> Facts {
        Facts { distro_id: "arch".into(), virt: "none".into(), user: "tester".into(), ..Default::default() }
    }

    fn oomd_file(catalog: &Catalog, n: usize) -> (String, String) {
        let f = &catalog.module("oomd").unwrap().files[n];
        (f.path.clone(), f.content.clone())
    }

    fn plan_earlyoom(f: &Facts, current: &dyn Fn(&str) -> Option<String>) -> Plan {
        let sel = Selection { include: vec!["earlyoom".into()], ..Default::default() };
        resolve(&Catalog::arch().unwrap(), f, &sel, current).unwrap()
    }

    #[test]
    fn choosing_a_module_retires_what_its_rival_left() {
        let catalog = Catalog::arch().unwrap();
        let (kept, _) = oomd_file(&catalog, 0);
        let (gone, content) = oomd_file(&catalog, 1);
        let mut f = facts();
        f.enabled_units.insert("systemd-oomd.service".into());
        let edited = format!("{MARKER}\n[Slice]\nManagedOOMSwap=auto\n");
        let current = |p: &str| match p {
            p if p == gone => Some(content.clone()),
            p if p == kept => Some(edited.clone()),
            _ => None,
        };
        let p = plan_earlyoom(&f, &current);
        let at = |want: &Step| p.steps.iter().position(|s| s == want).unwrap_or_else(|| panic!("{want:?}"));
        let disable = at(&Step::DisableUnits { units: vec!["systemd-oomd.service".into()] });
        at(&Step::RemoveFiles { paths: vec![gone.clone()] });
        let enable = p.steps.iter().position(|s| matches!(s, Step::EnableUnits { .. })).unwrap();
        assert!(disable < enable, "the rival stops before its replacement starts");
        assert!(p.notes.iter().any(|(id, n)| id == "earlyoom" && n.contains(&kept) && n.contains("edited")));
    }

    #[test]
    fn nothing_is_retired_on_a_clean_system_or_for_shared_units() {
        let p = plan_earlyoom(&facts(), &|_| None);
        assert!(!p.steps.iter().any(|s| matches!(s, Step::DisableUnits { .. } | Step::RemoveFiles { .. })));
        // scx-schedulers and scdr-sched-ext conflict but both enable scx_loader.
        let mut f = facts();
        f.enabled_units.insert("scx_loader.service".into());
        let sel = Selection { include: vec!["scdr-sched-ext".into()], ..Default::default() };
        let p = resolve(&Catalog::arch().unwrap(), &f, &sel, &|_| None).unwrap();
        assert!(p.modules.contains(&"scdr-sched-ext".to_string()));
        assert!(!p.steps.iter().any(|s| matches!(s, Step::DisableUnits { .. })));
    }
}
