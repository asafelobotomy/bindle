//! What the Choose screens show. The catalog side (`catalog`) never changes
//! while the wizard runs, so ticking a module only republishes `state`: the
//! lists keep their place instead of being rebuilt.

use std::collections::{BTreeMap, BTreeSet};

use bindle_core::catalog::{Module, Risk};
use bindle_run::present;
use serde_json::{Value, json};

use super::{Wizard, names};

/// What a click changed, for the line shown under the list.
#[derive(Debug, PartialEq)]
pub struct Change {
    pub text: String,
    /// `success`, `warning` or `muted`: how the line is tinted.
    pub tone: &'static str,
}

/// One entry of a group: a "choose one" set (keyed) or a single module.
type Item<'a> = (Option<&'a str>, Vec<Value>);

impl Wizard {
    /// Select or deselect a module. Selecting one of a set that can't be
    /// combined drops the others. `None`: the module can't be toggled here.
    pub fn toggle(&mut self, id: &str) -> Option<Change> {
        let m = self.catalog.module(id)?.clone();
        // Set up already, or the plan would skip it: nothing to toggle.
        if self.provided.contains_key(id) || self.facts.matches(&m.when).is_err() {
            return None;
        }
        if self.selected.remove(id) {
            let text = match self.needed().get(id) {
                Some(by) => format!("{} stays in the plan: {by} needs it", m.name),
                None => format!("Removed {}", m.name),
            };
            return Some(Change { text, tone: "muted" });
        }
        let before = self.needed();
        let dropped: Vec<&str> =
            m.conflicts.iter().filter(|r| self.selected.remove(r.as_str())).map(String::as_str).collect();
        self.selected.insert(id.to_string());
        if !dropped.is_empty() {
            let text =
                format!("{} replaces {}: only one of them can be installed", m.name, names(&self.catalog, &dropped));
            return Some(Change { text, tone: "warning" });
        }
        let pulled: Vec<String> = self.needed().into_keys().filter(|k| !before.contains_key(k)).collect();
        let pulled: Vec<&str> = pulled.iter().map(String::as_str).collect();
        let text = if pulled.is_empty() {
            format!("Added {}", m.name)
        } else {
            format!("Added {}, and {} which it needs", m.name, names(&self.catalog, &pulled))
        };
        Some(Change { text, tone: "success" })
    }

    pub(super) fn available(&self, m: &Module) -> bool {
        self.facts.matches(&m.when).is_ok()
    }

    /// Modules the plan adds because a selected one requires them:
    /// id → the name of a module that needs it.
    pub(super) fn needed(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        let mut queue: Vec<&Module> =
            self.selected.iter().filter_map(|id| self.catalog.module(id)).filter(|m| self.available(m)).collect();
        while let Some(m) = queue.pop() {
            for dep in &m.requires {
                let wanted = !self.selected.contains(dep) && !self.provided.contains_key(dep) && !out.contains_key(dep);
                if let Some(d) = self.catalog.module(dep).filter(|d| wanted && self.available(d)) {
                    out.insert(dep.clone(), m.name.clone());
                    queue.push(d);
                }
            }
        }
        out
    }

    /// Sets of modules that can't be combined (a firewall, a swap manager):
    /// module id → the set's key. The screen draws each set as "choose one".
    fn choices(&self) -> BTreeMap<&str, &str> {
        let mut key: BTreeMap<&str, &str> = BTreeMap::new();
        for m in self.catalog.modules.iter().filter(|m| !m.conflicts.is_empty()) {
            if key.contains_key(m.id.as_str()) {
                continue;
            }
            let mut queue = vec![m];
            while let Some(n) = queue.pop() {
                if key.insert(&n.id, &m.id).is_none() {
                    queue.extend(n.conflicts.iter().filter_map(|c| self.catalog.module(c)));
                }
            }
        }
        key
    }

    /// Every category: its groups in catalog order, each a list of items. An
    /// item is one module, or a "choose one" set (`choice`) of several.
    pub fn catalog(&self) -> Value {
        let choices = self.choices();
        let cats: Vec<Value> = self
            .catalog
            .categories
            .iter()
            .map(|c| {
                let mut groups: Vec<(&str, Vec<Item>)> = Vec::new();
                let mut total = 0;
                for m in self.catalog.modules_in(&c.id) {
                    total += 1;
                    let at = groups.iter().position(|(g, _)| *g == m.group).unwrap_or_else(|| {
                        groups.push((&m.group, Vec::new()));
                        groups.len() - 1
                    });
                    let items = &mut groups[at].1;
                    let choice = choices.get(m.id.as_str()).copied();
                    match items.iter_mut().find(|(k, _)| choice.is_some() && *k == choice) {
                        Some((_, set)) => set.push(self.module(m)),
                        None => items.push((choice, vec![self.module(m)])),
                    }
                }
                let groups: Vec<Value> = groups
                    .into_iter()
                    .map(|(name, items)| {
                        let items: Vec<Value> = items
                            .into_iter()
                            .map(|(choice, modules)| json!({ "choice": choice.is_some(), "modules": modules }))
                            .collect();
                        json!({ "name": name, "items": items })
                    })
                    .collect();
                json!({ "id": c.id, "name": c.name, "description": c.description, "total": total, "groups": groups })
            })
            .collect();
        json!(cats)
    }

    fn module(&self, m: &Module) -> Value {
        let risk = match m.risk {
            Risk::Safe => "",
            Risk::Moderate => "moderate",
            Risk::Advanced => "advanced",
        };
        let source = match (m.aur.is_empty(), m.releases.is_empty()) {
            (false, _) => "AUR",
            (true, false) => "GitHub release",
            (true, true) => "",
        };
        let ids = |v: &[String]| names(&self.catalog, &v.iter().map(String::as_str).collect::<Vec<_>>());
        json!({
            "id": m.id, "name": m.name, "description": m.description, "risk": risk, "source": source,
            "provided": self.provided.get(&m.id),
            "unavailable": self.facts.matches(&m.when).err(),
            "notes": m.notes,
            "details": present::module_details(m, &self.facts),
            "requires": ids(&m.requires),
            "conflicts": ids(&m.conflicts),
        })
    }

    /// What ticking changes: the selected modules, the ones the plan adds
    /// for them (`needed`), the profile's own picks (`suggested`) and counts.
    pub fn state(&self) -> Value {
        // A profile may include a module the plan skips here: show it off.
        let on: BTreeSet<&str> = self
            .selected
            .iter()
            .filter_map(|id| self.catalog.module(id))
            .filter(|m| self.available(m) && !self.provided.contains_key(&m.id))
            .map(|m| m.id.as_str())
            .collect();
        let counts: BTreeMap<&str, usize> = self
            .catalog
            .categories
            .iter()
            .map(|c| (c.id.as_str(), self.catalog.modules_in(&c.id).filter(|m| on.contains(m.id.as_str())).count()))
            .collect();
        let flags = |ids: &mut dyn Iterator<Item = &str>| -> BTreeMap<String, bool> {
            ids.map(|id| (id.to_string(), true)).collect()
        };
        let base = self.base();
        json!({
            "selected": flags(&mut on.iter().copied()),
            "needed": self.needed(),
            "suggested": flags(&mut base.iter().map(String::as_str)),
            "counts": counts,
            "total": on.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{arch, wizard};
    use super::*;

    fn find<'a>(cats: &'a Value, cat: &str, id: &str) -> (&'a Value, &'a Value) {
        let c = cats.as_array().unwrap().iter().find(|c| c["id"] == cat).unwrap();
        for g in c["groups"].as_array().unwrap() {
            for item in g["items"].as_array().unwrap() {
                if let Some(m) = item["modules"].as_array().unwrap().iter().find(|m| m["id"] == id) {
                    return (item, m);
                }
            }
        }
        panic!("{id} is not in {cat}");
    }

    #[test]
    fn toggling_a_rival_drops_the_other() {
        let mut w = wizard(arch());
        assert!(w.selected.contains("power-profiles"));
        let change = w.toggle("tlp").unwrap();
        assert!(change.text.contains("replaces Power profiles"), "{change:?}");
        assert_eq!(change.tone, "warning");
        assert!(w.selected.contains("tlp") && !w.selected.contains("power-profiles"));
        assert_eq!(w.toggle("tlp").unwrap().text, "Removed Laptop power tuning (TLP)");
        assert!(!w.selected.contains("tlp"));
        assert_eq!(w.toggle("no-such-module"), None);
    }

    #[test]
    fn a_click_says_what_else_it_brings_or_keeps() {
        let mut w = wizard(Facts { gpus: vec!["amd".into()], ..arch() });
        let added = w.toggle("steam").unwrap();
        assert!(added.text.starts_with("Added Steam, and ") && added.text.contains("multilib"), "{added:?}");
        let state = w.state();
        assert_eq!(state["needed"]["multilib"], "Steam");
        assert_eq!(state["selected"]["steam"], true);
        assert!(state["selected"].get("multilib").is_none());
        // Selecting the needed module itself stops it being "needed".
        assert_eq!(w.toggle("multilib").unwrap().text, "Added 32-bit packages (multilib)");
        assert!(w.state()["needed"].get("multilib").is_none());
        let kept = w.toggle("multilib").unwrap();
        assert!(kept.text.contains("stays in the plan: Steam needs it"), "{kept:?}");
    }

    #[test]
    fn rivals_share_one_choose_one_item() {
        let w = wizard(arch());
        let cats = w.catalog();
        let (item, tlp) = find(&cats, "hardware", "tlp");
        assert_eq!(item["choice"], true);
        let ids: Vec<&str> = item["modules"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["power-profiles", "tlp"]);
        assert_eq!(tlp["risk"], "moderate");
        assert_eq!(tlp["conflicts"], "Power profiles (power-profiles-daemon)");
        // A three-way set stays one item; a module without rivals is its own.
        let (sched, _) = find(&cats, "performance", "scx-schedulers");
        assert_eq!(sched["modules"].as_array().unwrap().len(), 3);
        let (alone, firefox) = find(&cats, "internet", "firefox");
        assert_eq!(alone["choice"], false);
        assert_eq!(firefox["source"], "");
        assert_eq!(find(&cats, "internet", "brave").1["source"], "AUR");
    }

    #[test]
    fn groups_keep_catalog_order_and_hold_every_module() {
        let w = wizard(arch());
        let cats = w.catalog();
        let net = cats.as_array().unwrap().iter().find(|c| c["id"] == "internet").unwrap();
        let groups = net["groups"].as_array().unwrap();
        assert_eq!(groups[0]["name"], "Browsers");
        let held: usize = groups
            .iter()
            .flat_map(|g| g["items"].as_array().unwrap())
            .map(|i| i["modules"].as_array().unwrap().len())
            .sum();
        assert_eq!(held as u64, net["total"].as_u64().unwrap());
    }

    #[test]
    fn modules_that_dont_apply_here_are_not_ticked() {
        // Reflector needs Arch; the desktop profile includes it.
        let mut w = wizard(Facts { distro_id: "manjaro".into(), ..arch() });
        assert!(w.selected.contains("mirrors"));
        let cats = w.catalog();
        assert!(find(&cats, "base", "mirrors").1["unavailable"].is_string());
        let state = w.state();
        assert!(state["selected"].get("mirrors").is_none());
        assert_eq!(state["suggested"]["mirrors"], true);
        let counted: u64 = state["counts"].as_object().unwrap().values().map(|n| n.as_u64().unwrap()).sum();
        assert_eq!(counted, state["total"].as_u64().unwrap());
        assert_eq!(counted as usize, state["selected"].as_object().unwrap().len());
        assert_eq!(w.toggle("mirrors"), None);
        assert!(w.selected.contains("mirrors"), "toggling an unavailable module changes nothing");
    }

    #[test]
    fn detected_drives_are_cards_of_their_own_group_with_the_line_they_add() {
        use bindle_core::detect::Drive;
        let drive = |uuid: &str, fstype: &str, blocked: Option<&str>| Drive {
            device: "/dev/sdb1".into(),
            uuid: uuid.into(),
            fstype: fstype.into(),
            label: "Data".into(),
            size: 500 << 30,
            blocked: blocked.map(String::from),
            ..Default::default()
        };
        let facts = Facts {
            drives: vec![drive("aa-1", "ext4", None), drive("bb", "ntfs", Some("already in /etc/fstab"))],
            ..arch()
        };
        let mut w = Wizard::new(bindle_core::Catalog::arch().unwrap().with_drives(&facts), facts);
        let cats = w.catalog();
        let hw = cats.as_array().unwrap().iter().find(|c| c["id"] == "hardware").unwrap();
        let group = hw["groups"].as_array().unwrap().last().unwrap();
        assert_eq!(group["name"], "Drives to mount at startup");
        assert_eq!(group["items"].as_array().unwrap().len(), 2);
        let (_, data) = find(&cats, "hardware", "mount-aa-1");
        assert_eq!(data["name"], "Mount Data (500 GiB ext4)");
        assert!(data["description"].as_str().unwrap().contains("Mounted at /mnt/Data"));
        let details = data["details"].as_array().unwrap();
        assert!(
            details.iter().any(|d| d.as_str().unwrap().starts_with("Adds to /etc/fstab: UUID=aa-1 /mnt/Data ext4"))
        );
        assert_eq!(find(&cats, "hardware", "mount-bb").1["unavailable"], "already in /etc/fstab");
        // Not chosen for anyone: ticking it is the user's call, and the review shows the line.
        assert!(w.state()["selected"].get("mount-aa-1").is_none());
        assert_eq!(w.toggle("mount-aa-1").unwrap().text, "Added Mount Data (500 GiB ext4)");
        assert_eq!(w.toggle("mount-bb"), None);
        let review = w.review(std::path::Path::new("/nonexistent"));
        let step =
            review["steps"].as_array().unwrap().iter().find(|s| s["text"].as_str().unwrap().contains("/etc/fstab"));
        let step = step.unwrap_or_else(|| panic!("{review}"));
        assert!(step["content"][0].as_str().unwrap().starts_with("UUID=aa-1  /mnt/Data  ext4  defaults,nofail"));
    }

    use bindle_core::Facts;
}
