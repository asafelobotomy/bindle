//! What a module puts on this system, resolved: its packages (with a
//! builtin's computed ones), files, units, groups and commands. Recorded at
//! every apply, so a newer Bindle can say what changed for each module
//! (`whatsnew`). Mount points (`drives`) are left out: they are the user's
//! own choice of drives, not catalog content.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::builtins::Expansion;
use crate::catalog::Module;
use crate::detect::Facts;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Content {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aur: Vec<String>,
    /// `name tag` of each pinned release asset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub releases: Vec<String>,
    /// Path → content of each file the module writes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    /// Commands whose conditions hold here: `# argv` (root) or `$ argv` (user).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,
    /// pacman.conf options and repositories.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pacman: Vec<String>,
}

fn sorted(v: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = v.into_iter().collect();
    v.sort();
    v.dedup();
    v
}

/// `m`'s content on this system (`e`: its builtin's expansion).
pub fn of(m: &Module, e: Option<&Expansion>, facts: &Facts) -> Content {
    let holds = |c: &&crate::catalog::Command| facts.matches(&c.when).is_ok();
    let mut files: BTreeMap<String, String> = m.files.iter().map(|f| (f.path.clone(), f.content.clone())).collect();
    let mut commands: Vec<String> =
        m.root_commands.iter().filter(holds).map(|c| format!("# {}", c.argv.join(" "))).collect();
    commands.extend(m.user_commands.iter().filter(holds).map(|c| format!("$ {}", c.argv.join(" "))));
    let mut packages = m.packages.clone();
    let mut aur = m.aur.clone();
    let mut units = m.enable.clone();
    if let Some(e) = e {
        packages.extend(e.packages.iter().cloned());
        aur.extend(e.aur.iter().cloned());
        units.extend(e.enable.iter().cloned());
        files.extend(e.files.iter().map(|f| (f.path.clone(), f.content.clone())));
    }
    let pacman = m.pacman.iter().map(|(k, v)| if v.is_empty() { k.clone() } else { format!("{k} = {v}") });
    Content {
        packages: sorted(packages),
        aur: sorted(aur),
        releases: sorted(m.releases.iter().map(|r| format!("{} {}", r.name, r.tag))),
        files,
        units: sorted(units),
        groups: sorted(m.groups.iter().cloned()),
        commands,
        pacman: sorted(pacman.chain(m.repos.iter().map(|r| format!("[{r}]")))),
    }
}
