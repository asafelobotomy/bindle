//! The wizard's state and the data each screen shows, as JSON for QML.
//! No Qt here: the rules match the terminal wizard (`bindle-cli/src/tui`).

use std::collections::{BTreeMap, BTreeSet};

use bindle_core::{Catalog, Facts, Plan, Selection, Step, plan};
use bindle_run::present;
use serde_json::{Value, json};

mod choose;

pub struct Wizard {
    catalog: Catalog,
    facts: Facts,
    profile: String,
    selected: BTreeSet<String>,
    provided: BTreeMap<String, String>,
}

impl Wizard {
    pub fn new(catalog: Catalog, facts: Facts) -> Self {
        let profile = if facts.desktops.is_empty() { "minimal" } else { "desktop" }.to_string();
        let provided = facts.install.provided();
        let mut w = Wizard { catalog, facts, profile, selected: BTreeSet::new(), provided };
        w.selected = w.base();
        w
    }

    /// Start from a saved selection (the one last applied or tried) instead
    /// of the suggested profile. Modules the catalog no longer has are dropped.
    pub fn seed(&mut self, sel: &Selection) {
        if let Some(p) = &sel.profile {
            self.set_profile(p);
        }
        let known: Vec<String> = sel.include.iter().filter(|id| self.catalog.module(id).is_some()).cloned().collect();
        self.selected.extend(known);
        self.selected.retain(|id| !sel.exclude.contains(id));
    }

    fn base(&self) -> BTreeSet<String> {
        let sel = Selection { profile: Some(self.profile.clone()), ..Default::default() };
        sel.initial(&self.catalog).unwrap_or_default()
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Start over from a profile's modules.
    pub fn set_profile(&mut self, id: &str) {
        if self.catalog.profiles.iter().any(|p| p.id == id) {
            self.profile = id.to_string();
            self.selected = self.base();
        }
    }

    pub fn selection(&self) -> Selection {
        let base = self.base();
        Selection {
            profile: Some(self.profile.clone()),
            include: self.selected.difference(&base).cloned().collect(),
            exclude: base.difference(&self.selected).cloned().collect(),
            // The review lists replacements before anything happens.
            replace_conflicts: true,
        }
    }

    /// One line under the title: distro, CPU, RAM, GPUs, form factor.
    pub fn header(&self) -> String {
        let f = &self.facts;
        let vm = f.virt != "none" && !f.virt.is_empty();
        // A VM's "virtual" GPU says nothing "virtual machine" doesn't.
        let gpus: Vec<&str> = f.gpus.iter().map(String::as_str).filter(|g| !(vm && *g == "virtual")).collect();
        let mut parts =
            vec![f.pretty_name.clone(), f.cpu_vendor.clone(), format!("{:.0} GiB", f.ram_mib as f64 / 1024.0)];
        match (gpus.is_empty(), vm) {
            (false, _) => parts.push(gpus.join(" + ")),
            (true, false) => parts.push("no GPU".into()),
            (true, true) => {}
        }
        parts.push(
            if vm {
                "virtual machine"
            } else if f.laptop {
                "laptop"
            } else {
                "desktop PC"
            }
            .into(),
        );
        parts.join(" · ")
    }

    pub fn welcome(&self) -> Value {
        json!(present::install_summary(&self.facts))
    }

    pub fn profiles(&self) -> Value {
        let profiles: Vec<Value> = self
            .catalog
            .profiles
            .iter()
            .map(|p| {
                let picks = Selection { profile: Some(p.id.clone()), ..Default::default() }
                    .initial(&self.catalog)
                    .unwrap_or_default();
                // What the profile holds, by category: the card shows it. Only
                // what can be ticked here counts, as on the screens that follow.
                let here = |m: &&bindle_core::catalog::Module| {
                    picks.contains(&m.id) && self.available(m) && !self.provided.contains_key(&m.id)
                };
                let holds: Vec<Value> = self
                    .catalog
                    .categories
                    .iter()
                    .filter_map(|c| {
                        let n = self.catalog.modules_in(&c.id).filter(here).count();
                        (n > 0).then(|| json!({ "id": c.id, "name": c.name, "count": n }))
                    })
                    .collect();
                json!({
                    "id": p.id, "name": p.name, "description": p.description,
                    "count": holds.iter().map(|h| h["count"].as_u64().unwrap_or(0)).sum::<u64>(), "holds": holds,
                })
            })
            .collect();
        json!(profiles)
    }

    pub fn facts(&self) -> &Facts {
        &self.facts
    }

    /// The plan for the current selection, reading files from `root`
    /// (`/`, or the sandbox).
    pub fn plan(&self, root: &std::path::Path) -> anyhow::Result<Plan> {
        let read = |p: &str| std::fs::read_to_string(bindle_run::sys::host(root, p)).ok();
        plan::resolve(&self.catalog, &self.facts, &self.selection(), &read)
    }

    pub fn review(&self, root: &std::path::Path) -> Value {
        match self.plan(root) {
            Ok(p) => review_json(&p, &self.catalog),
            Err(e) => json!({ "ok": false, "error": format!("{e:#}") }),
        }
    }
}

fn names(catalog: &Catalog, ids: &[&str]) -> String {
    let names: Vec<&str> = ids.iter().map(|id| catalog.module(id).map_or(*id, |m| m.name.as_str())).collect();
    names.join(", ")
}

/// The planned modules by category, in catalog order: what the summary cards show.
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

fn review_json(p: &Plan, catalog: &Catalog) -> Value {
    let name = |id: &str| catalog.module(id).map_or(id.to_string(), |m| m.name.clone());
    let mut packages = 0;
    let (mut files, mut services) = (0, 0);
    for s in &p.steps {
        match s {
            Step::InstallPackages { packages: p } | Step::InstallAur { packages: p } => packages += p.len(),
            Step::InstallReleases { assets } => packages += assets.len(),
            Step::BootstrapAurHelper { .. } => packages += 1,
            Step::WriteFile { .. } | Step::MountAtBoot { .. } => files += 1,
            Step::EnableUnits { units } => services += units.len(),
            _ => {}
        }
    }
    let steps: Vec<Value> = p
        .steps
        .iter()
        .map(|s| json!({ "root": s.privileged(), "text": present::describe(s), "content": present::written(s) }))
        .collect();
    let pair = |v: &[(String, String)], f: &dyn Fn(&str, &str) -> String| -> Vec<String> {
        v.iter().map(|(a, b)| f(a, b)).collect()
    };
    json!({
        "ok": true,
        "empty": p.is_empty(),
        "groups": groups(p, catalog),
        "needsReboot": bindle_run::apply::needs_reboot(p),
        "relogin": p.steps.iter().any(|s| matches!(s, Step::AddToGroups { .. })),
        "password": p.steps.iter().any(Step::privileged),
        "counts": { "modules": p.modules.len(), "packages": packages, "files": files, "services": services },
        "replaced": pair(&p.replaced, &|id, pkg| format!("{} replaces the installed {pkg}", name(id))),
        "pulled_in": pair(&p.pulled_in, &|dep, by| format!("{} added: {} needs it", name(dep), name(by))),
        "provided": pair(&p.provided, &|id, why| format!("{}: {why}", name(id))),
        "skipped": pair(&p.skipped, &|id, why| format!("{} skipped: {why}", name(id))),
        "steps": steps,
        "notes": pair(&p.notes, &|id, n| format!("{}: {n}", name(id))),
    })
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub fn wizard(f: Facts) -> Wizard {
        Wizard::new(Catalog::arch().unwrap(), f)
    }

    pub fn arch() -> Facts {
        Facts { distro_id: "arch".into(), desktops: vec!["gnome".into()], laptop: true, ..Default::default() }
    }

    #[test]
    fn starts_from_the_desktop_profile_on_a_desktop() {
        let w = wizard(arch());
        assert_eq!(w.profile(), "desktop");
        assert_eq!(wizard(Facts::default()).profile(), "minimal");
        let sel = w.selection();
        assert!(sel.include.is_empty() && sel.exclude.is_empty());
    }

    #[test]
    fn choosing_a_profile_resets_the_selection() {
        let mut w = wizard(arch());
        w.toggle("blender");
        w.set_profile("gaming");
        assert!(w.selected.contains("steam") && !w.selected.contains("blender"));
        w.set_profile("nonsense");
        assert_eq!(w.profile(), "gaming");
    }

    #[test]
    fn screens_get_the_fields_they_show() {
        let w = wizard(arch());
        let profiles = w.profiles();
        for p in profiles.as_array().unwrap() {
            let held: u64 = p["holds"].as_array().unwrap().iter().map(|h| h["count"].as_u64().unwrap()).sum();
            assert!(held > 0 && held == p["count"].as_u64().unwrap(), "{p}");
        }
        assert!(w.header().contains("laptop"));
        let vm = wizard(Facts { virt: "kvm".into(), gpus: vec!["virtual".into()], ..arch() });
        assert!(vm.header().ends_with("GiB · virtual machine"), "{}", vm.header());
        assert!(w.welcome()["rows"].is_array());
    }

    #[test]
    fn review_counts_what_changes() {
        let mut w = wizard(arch());
        w.toggle("blender");
        let r = w.review(std::path::Path::new("/"));
        assert_eq!(r["ok"], true, "{r}");
        assert!(r["counts"]["packages"].as_u64().unwrap() > 0);
        let steps = r["steps"].as_array().unwrap();
        assert!(steps.iter().any(|s| s["text"].as_str().unwrap().contains("blender")));
        assert!(steps.iter().any(|s| s["root"] == true));
        assert_eq!(r["password"], true);
        assert!(r["needsReboot"].is_boolean() && r["relogin"].is_boolean());
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
        // Catalog order, empty categories left out.
        let ids: Vec<&str> = groups.iter().map(|g| g["id"].as_str().unwrap()).collect();
        assert!(!ids.contains(&"gaming"), "{ids:?}");
    }
}
