//! The Profile screen: a base profile (Minimal, Desktop) and add-ons ticked
//! on top of it (Gaming, Developer: `addon = true` in profiles.toml).

use std::collections::BTreeSet;

use bindle_core::Selection;
use bindle_core::catalog::Module;
use serde_json::{Value, json};

use super::Wizard;

impl Wizard {
    /// What a profile pre-selects on its own (`Selection::initial`).
    pub(super) fn profile_set(&self, id: &str) -> BTreeSet<String> {
        let sel = Selection { profile: Some(id.to_string()), ..Default::default() };
        sel.initial(&self.catalog).unwrap_or_default()
    }

    fn is_addon(&self, id: &str) -> bool {
        self.catalog.profiles.iter().any(|p| p.id == id && p.addon)
    }

    /// An add-on's own modules: those that list it and no base profile.
    fn extras(&self, addon: &str) -> BTreeSet<String> {
        let bases: Vec<&str> = self.catalog.profiles.iter().filter(|p| !p.addon).map(|p| p.id.as_str()).collect();
        self.catalog
            .modules
            .iter()
            .filter(|m| {
                !m.default && m.profiles.iter().any(|p| p == addon) && !m.profiles.iter().any(|p| bases.contains(&&**p))
            })
            .map(|m| m.id.clone())
            .collect()
    }

    /// The suggested set: the base profile's modules plus each add-on's.
    pub(super) fn base(&self) -> BTreeSet<String> {
        let mut set = self.profile_set(&self.profile);
        for a in &self.addons {
            set.extend(self.extras(a));
        }
        set
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Start over from a base profile, keeping the ticked add-ons. An add-on
    /// id ticks or unticks that add-on instead.
    pub fn set_profile(&mut self, id: &str) {
        if self.is_addon(id) {
            self.toggle_addon(id);
        } else if self.catalog.profiles.iter().any(|p| p.id == id) {
            self.profile = id.to_string();
            self.selected = self.base();
        }
    }

    /// Add or take away an add-on's own modules, leaving every other pick.
    pub fn toggle_addon(&mut self, id: &str) {
        if !self.is_addon(id) {
            return;
        }
        let extras = self.extras(id);
        if self.addons.remove(id) {
            let still = self.base();
            self.selected.retain(|m| !extras.contains(m) || still.contains(m));
        } else {
            self.addons.insert(id.to_string());
            self.selected.extend(extras);
        }
    }

    /// The selection differs from the profile's suggestion: changing the
    /// base profile would throw those edits away, so the screen asks first.
    pub fn edited(&self) -> bool {
        self.selected != self.base()
    }

    /// Start from a saved selection (the one last applied or tried) instead
    /// of the suggested profile. Modules the catalog no longer has are dropped.
    pub fn seed(&mut self, sel: &Selection) {
        match sel.profile.as_deref() {
            // Saved by the terminal wizard, where an add-on is a whole profile.
            Some(p) if self.is_addon(p) => {
                self.addons.insert(p.to_string());
                self.selected = self.base();
            }
            Some(p) => self.set_profile(p),
            None => {}
        }
        let known: Vec<String> = sel.include.iter().filter(|id| self.catalog.module(id).is_some()).cloned().collect();
        self.selected.extend(known);
        self.selected.retain(|id| !sel.exclude.contains(id));
        // An add-on whose own modules are all picked shows as ticked.
        let addons: Vec<String> = self.catalog.profiles.iter().filter(|p| p.addon).map(|p| p.id.clone()).collect();
        for a in addons {
            let extras = self.extras(&a);
            if !extras.is_empty() && extras.is_subset(&self.selected) {
                self.addons.insert(a);
            }
        }
    }

    fn here(&self, m: &Module) -> bool {
        self.available(m) && !self.provided.contains_key(&m.id)
    }

    /// Bases and add-ons, each with what it pre-selects here.
    pub fn profiles(&self) -> Value {
        let card = |id: &str, name: &str, description: &str, picks: &BTreeSet<String>| {
            // Only what can be ticked here counts, as on the screens that follow.
            let mods: Vec<&Module> = self
                .catalog
                .categories
                .iter()
                .flat_map(|c| self.catalog.modules_in(&c.id))
                .filter(|m| picks.contains(&m.id) && self.here(m))
                .collect();
            let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
            json!({ "id": id, "name": name, "description": description, "count": mods.len(), "names": names })
        };
        let bases: Vec<Value> = self
            .catalog
            .profiles
            .iter()
            .filter(|p| !p.addon)
            .map(|p| card(&p.id, &p.name, &p.description, &self.profile_set(&p.id)))
            .collect();
        let addons: Vec<Value> = self
            .catalog
            .profiles
            .iter()
            .filter(|p| p.addon)
            .map(|p| {
                let mut c = card(&p.id, &p.name, &p.description, &self.extras(&p.id));
                c["on"] = json!(self.addons.contains(&p.id));
                c
            })
            .collect();
        json!({ "bases": bases, "addons": addons, "base": self.profile, "edited": self.edited() })
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{arch, wizard};

    #[test]
    fn choosing_a_base_resets_the_selection_but_keeps_add_ons() {
        let mut w = wizard(arch());
        w.toggle_addon("gaming");
        w.toggle("blender");
        assert!(w.edited());
        w.set_profile("minimal");
        assert!(w.selected.contains("steam") && !w.selected.contains("blender"));
        assert!(!w.selected.contains("flatpak"), "Desktop's picks went with it");
        assert!(!w.edited());
        w.set_profile("nonsense");
        assert_eq!(w.profile(), "minimal");
    }

    #[test]
    fn an_add_on_adds_and_removes_only_its_own_modules() {
        let mut w = wizard(arch());
        w.toggle("blender");
        w.toggle_addon("gaming");
        assert!(w.selected.contains("steam") && w.selected.contains("multilib"));
        let p = w.profiles();
        let gaming = p["addons"].as_array().unwrap().iter().find(|a| a["id"] == "gaming").unwrap();
        assert_eq!(gaming["on"], true);
        assert!(gaming["names"].as_array().unwrap().iter().any(|n| n == "Steam"), "{gaming}");
        assert!(!gaming["names"].as_array().unwrap().iter().any(|n| n == "Fonts"), "{gaming}");
        w.toggle_addon("gaming");
        assert!(!w.selected.contains("steam") && w.selected.contains("blender"));
        // Saved as Desktop plus includes, which the CLI understands.
        w.toggle_addon("developer");
        let sel = w.selection();
        assert_eq!(sel.profile.as_deref(), Some("desktop"));
        assert!(sel.include.contains(&"neovim".to_string()));
    }

    #[test]
    fn a_saved_add_on_profile_seeds_as_base_plus_add_on() {
        let mut w = wizard(arch());
        let sel = bindle_core::Selection { profile: Some("gaming".into()), ..Default::default() };
        w.seed(&sel);
        assert_eq!(w.profile(), "desktop");
        assert!(w.addons.contains("gaming") && w.selected.contains("steam"));
        let p = w.profiles();
        assert_eq!(p["bases"].as_array().unwrap().len(), 2);
        for b in p["bases"].as_array().unwrap() {
            assert_eq!(b["count"].as_u64().unwrap() as usize, b["names"].as_array().unwrap().len());
        }
    }
}
