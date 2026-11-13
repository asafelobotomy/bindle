//! The Review screen: what applying will cost (password, restart, undo),
//! then three lists: what gets installed, what gets changed, and what is
//! left alone and why. Every step, with its files, is under the details.

use bindle_core::{Catalog, Plan, Step};
use bindle_run::present;
use serde_json::{Value, json};

/// The planned modules by category, in catalog order.
fn groups(p: &Plan, catalog: &Catalog) -> Vec<Value> {
    catalog
        .categories
        .iter()
        .filter_map(|c| {
            let names: Vec<&str> =
                catalog.modules_in(&c.id).filter(|m| p.modules.contains(&m.id)).map(|m| m.name.as_str()).collect();
            (!names.is_empty()).then(|| json!({ "id": c.id, "name": c.name, "modules": names }))
        })
        .collect()
}

/// `{what, why}` rows for the Install and Change lists.
fn row(what: impl Into<String>, why: impl Into<String>) -> Value {
    json!({ "what": what.into(), "why": why.into() })
}

pub fn json(p: &Plan, catalog: &Catalog) -> Value {
    let name = |id: &str| catalog.module(id).map_or(id.to_string(), |m| m.name.clone());
    let (mut install, mut change) = (Vec::new(), Vec::new());
    let (mut files, mut services) = (0, 0);
    for s in &p.steps {
        match s {
            Step::InstallPackages { packages } => install.extend(packages.iter().map(|k| row(k, "from Arch"))),
            Step::InstallAur { packages } => install.extend(packages.iter().map(|k| row(k, "from the AUR"))),
            Step::BootstrapAurHelper { package } => install.push(row(package, "the AUR helper, built first")),
            Step::InstallReleases { assets } => {
                install.extend(assets.iter().map(|a| row(&a.name, format!("from {} on GitHub", a.repo))))
            }
            Step::RemovePackages { packages } => {
                change.extend(packages.iter().map(|k| row(format!("remove {k}"), "replaced by what you chose")))
            }
            Step::WriteFile { module, file } => {
                files += 1;
                change.push(row(&file.path, name(module)));
            }
            Step::MountAtBoot { entries } => {
                files += 1;
                for e in entries {
                    change.push(row("/etc/fstab", format!("mount at {} on startup", e.dir)));
                }
            }
            Step::EditPacmanConf { .. } => change.push(row("/etc/pacman.conf", "pacman options and repositories")),
            Step::EnableUnits { units } => {
                services += units.len();
                change.extend(units.iter().map(|u| row(u, "turned on")));
            }
            Step::DisableUnits { units } => change.extend(units.iter().map(|u| row(u, "turned off: replaced"))),
            Step::RemoveFiles { paths } => change.extend(paths.iter().map(|f| row(f, "removed: replaced"))),
            Step::AddToGroups { user, groups } => change.push(row(format!("groups for {user}"), groups.join(", "))),
            Step::RebuildInitramfs => change.push(row("initramfs", "rebuilt for the new kernel or drivers")),
            Step::RunAsRoot { module, argv } | Step::RunAsUser { module, argv, .. } => {
                change.push(row(argv.join(" "), format!("run for {}", name(module))))
            }
        }
    }
    let steps: Vec<Value> = p
        .steps
        .iter()
        .map(|s| json!({ "root": s.privileged(), "text": present::describe(s), "content": present::written(s) }))
        .collect();
    let pair = |v: &[(String, String)]| -> Vec<Value> { v.iter().map(|(id, why)| row(name(id), why)).collect() };
    json!({
        "ok": true,
        "empty": p.is_empty(),
        "groups": groups(p, catalog),
        "needsReboot": bindle_run::apply::needs_reboot(p),
        "relogin": p.steps.iter().any(|s| matches!(s, Step::AddToGroups { .. })),
        "password": p.steps.iter().any(Step::privileged),
        "counts": { "modules": p.modules.len(), "packages": install.len(), "files": files, "services": services },
        "install": install,
        "change": change,
        // Left alone: set up already, not for this computer, or kept as it is.
        "provided": pair(&p.provided),
        "skipped": pair(&p.skipped),
        "notes": pair(&p.notes),
        "replaced": p.replaced.iter().map(|(id, pkg)| format!("{} replaces the installed {pkg}", name(id))).collect::<Vec<_>>(),
        "pulled_in": p.pulled_in.iter().map(|(dep, by)| format!("{} added: {} needs it", name(dep), name(by))).collect::<Vec<_>>(),
        "steps": steps,
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::{arch, wizard};

    #[test]
    fn review_sorts_the_plan_into_install_and_change() {
        let mut w = wizard(arch());
        w.toggle("blender");
        let r = w.review(std::path::Path::new("/"));
        assert_eq!(r["ok"], true, "{r}");
        let install = r["install"].as_array().unwrap();
        assert!(install.iter().any(|i| i["what"] == "blender" && i["why"] == "from Arch"), "{install:?}");
        assert_eq!(r["counts"]["packages"].as_u64().unwrap() as usize, install.len());
        assert!(r["change"].as_array().unwrap().iter().all(|c| c["what"].is_string() && c["why"].is_string()));
        let steps = r["steps"].as_array().unwrap();
        assert!(steps.iter().any(|s| s["root"] == true));
        assert_eq!(r["password"], true);
        assert!(r["needsReboot"].is_boolean() && r["relogin"].is_boolean());
        assert!(r["provided"].is_array() && r["skipped"].is_array() && r["notes"].is_array());
    }

    #[test]
    fn review_groups_every_planned_module_by_category() {
        let mut w = wizard(arch());
        w.toggle("blender");
        let r = w.review(std::path::Path::new("/"));
        let groups = r["groups"].as_array().unwrap();
        let total: usize = groups.iter().map(|g| g["modules"].as_array().unwrap().len()).sum();
        assert_eq!(total as u64, r["counts"]["modules"].as_u64().unwrap());
        let creative = groups.iter().find(|g| g["id"] == "creative").unwrap();
        assert!(creative["modules"].as_array().unwrap().iter().any(|m| m == "Blender"), "{creative}");
        let ids: Vec<&str> = groups.iter().map(|g| g["id"].as_str().unwrap()).collect();
        assert!(!ids.contains(&"gaming"), "{ids:?}");
    }
}
