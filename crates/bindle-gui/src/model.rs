//! The wizard's state and the data each screen shows, as JSON for QML.
//! No Qt here: the rules match the terminal wizard (`bindle-cli/src/tui`).

use std::collections::{BTreeMap, BTreeSet};

use bindle_core::{Catalog, Facts, Plan, Selection, plan};
use bindle_run::present;
use serde_json::{Value, json};

mod choose;
pub use choose::Change;
mod profile;
mod review;
mod whatsnew;

pub struct Wizard {
    catalog: Catalog,
    facts: Facts,
    /// The base profile (Minimal, Desktop): `Selection::profile`.
    profile: String,
    /// Add-on profiles ticked on top of it (Gaming, Developer).
    addons: BTreeSet<String>,
    selected: BTreeSet<String>,
    provided: BTreeMap<String, String>,
    /// "What's new" after a Bindle upgrade, when there is any.
    whats_new: Option<whatsnew::WhatsNewState>,
}

impl Wizard {
    pub fn new(catalog: Catalog, facts: Facts) -> Self {
        let profile = if facts.desktops.is_empty() { "minimal" } else { "desktop" }.to_string();
        let provided = facts.install.provided();
        let mut w = Wizard {
            catalog,
            facts,
            profile,
            addons: BTreeSet::new(),
            selected: BTreeSet::new(),
            provided,
            whats_new: None,
        };
        w.selected = w.base();
        w
    }

    pub fn selection(&self) -> Selection {
        if let Some(w) = self.active_whats_new() {
            return self.whats_new_selection(w);
        }
        // Relative to the base profile alone: add-ons travel as includes,
        // which every frontend (and `bindle apply --from`) understands.
        let base = self.profile_set(&self.profile);
        Selection {
            profile: Some(self.profile.clone()),
            include: self.selected.difference(&base).cloned().collect(),
            exclude: base.difference(&self.selected).cloned().collect(),
            // The review lists replacements before anything happens.
            replace_conflicts: true,
            exact: false,
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

    /// The installer's summary, plus what already works by module name.
    pub fn welcome(&self) -> Value {
        let summary = present::install_summary(&self.facts);
        let working: Vec<Value> = summary
            .provided
            .iter()
            .map(|(id, why)| {
                let name = self.catalog.module(id).map_or(id.as_str(), |m| m.name.as_str());
                json!({ "name": name, "how": why })
            })
            .collect();
        let mut v = json!(summary);
        v["working"] = json!(working);
        v
    }

    pub fn facts(&self) -> &Facts {
        &self.facts
    }

    /// The plan for the current selection, reading files from `root`
    /// (`/`, or the sandbox).
    pub fn plan(&self, root: &std::path::Path) -> anyhow::Result<Plan> {
        if let Some(w) = self.active_whats_new() {
            return self.whats_new_plan(w, root);
        }
        let read = |p: &str| std::fs::read_to_string(bindle_run::sys::host(root, p)).ok();
        plan::resolve(&self.catalog, &self.facts, &self.selection(), &read)
    }

    pub fn review(&self, root: &std::path::Path) -> Value {
        match self.plan(root) {
            Ok(p) => review::json(&p, &self.catalog),
            Err(e) => json!({ "ok": false, "error": format!("{e:#}") }),
        }
    }
}

fn names(catalog: &Catalog, ids: &[&str]) -> String {
    let names: Vec<&str> = ids.iter().map(|id| catalog.module(id).map_or(*id, |m| m.name.as_str())).collect();
    names.join(", ")
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
    fn screens_get_the_fields_they_show() {
        let w = wizard(arch());
        assert!(w.header().contains("laptop"));
        let vm = wizard(Facts { virt: "kvm".into(), gpus: vec!["virtual".into()], ..arch() });
        assert!(vm.header().ends_with("GiB · virtual machine"), "{}", vm.header());
        let welcome = w.welcome();
        assert!(welcome["rows"].is_array() && welcome["working"].is_array());
    }
}
