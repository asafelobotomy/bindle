//! Declarative module catalog. Categories live in `catalog/<distro>/*.toml`
//! and are embedded at build time so the AppImage is self-contained.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

mod validate;

const ARCH_FILES: &[(&str, &str)] = &[
    ("base", include_str!("../../../catalog/arch/base.toml")),
    ("hardware", include_str!("../../../catalog/arch/hardware.toml")),
    ("performance", include_str!("../../../catalog/arch/performance.toml")),
    ("desktop", include_str!("../../../catalog/arch/desktop.toml")),
    ("internet", include_str!("../../../catalog/arch/internet.toml")),
    ("creative", include_str!("../../../catalog/arch/creative.toml")),
    ("development", include_str!("../../../catalog/arch/development.toml")),
    ("ai", include_str!("../../../catalog/arch/ai.toml")),
    ("gaming", include_str!("../../../catalog/arch/gaming.toml")),
    ("security", include_str!("../../../catalog/arch/security.toml")),
];
const PROFILES: &str = include_str!("../../../catalog/profiles.toml");

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    #[default]
    Safe,
    Moderate,
    Advanced,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub order: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// One asset of a GitHub release, pinned by tag and sha256: a prebuilt Arch
/// package (`.pkg.tar.zst`, installed with `pacman -U`) or an AppImage
/// (installed for the user in ~/.local/bin with a desktop entry).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseAsset {
    /// `owner/repo` on GitHub.
    pub repo: String,
    pub tag: String,
    /// File name of the asset in that release.
    pub asset: String,
    pub sha256: String,
    /// Package name (packages) or command name (AppImages).
    pub name: String,
    /// Shown in the AppImage's desktop entry; filled from the module name.
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Package,
    AppImage,
}

impl ReleaseAsset {
    pub fn kind(&self) -> Option<AssetKind> {
        if self.asset.ends_with(".pkg.tar.zst") {
            Some(AssetKind::Package)
        } else if self.asset.ends_with(".AppImage") {
            Some(AssetKind::AppImage)
        } else {
            None
        }
    }

    pub fn url(&self) -> String {
        format!("https://github.com/{}/releases/download/{}/{}", self.repo, self.tag, self.asset)
    }

    /// Desktop entry an installed AppImage gets; it records the asset, so a
    /// newer pinned release is planned again.
    pub fn desktop_entry(&self) -> String {
        format!("~/.local/share/applications/bindle-{}.desktop", self.name)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileSpec {
    pub path: String,
    pub content: String,
    #[serde(default = "default_mode")]
    pub mode: u32,
}

fn default_mode() -> u32 {
    0o644
}

/// A file condition, read through the planner's current-file reader. Holds
/// when the file is readable and, if given, `contains` appears in it and it
/// equals `content` (a unit masked via /dev/null reads as `content = ""`).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileCheck {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl FileCheck {
    pub fn holds(&self, text: Option<&str>) -> bool {
        text.is_some_and(|t| {
            self.contains.as_deref().is_none_or(|c| t.contains(c)) && self.content.as_deref().is_none_or(|c| t == c)
        })
    }
}

/// A root or user command: either a bare argv array or a table with
/// `when` tags and `unless` checks (all must hold to skip it as done).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(from = "CommandRepr")]
pub struct Command {
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub when: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unless: Vec<FileCheck>,
    /// Exit codes (as strings) that mean "done, but the user must finish
    /// something": the apply goes on and the message is shown at the end.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub manual: BTreeMap<String, String>,
    /// Run before the AUR builds (a user command they need, such as
    /// `rustup default stable`), not after everything else.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub before_aur: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CommandRepr {
    Argv(Vec<String>),
    Full(FullCommand),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FullCommand {
    argv: Vec<String>,
    #[serde(default)]
    when: Vec<String>,
    #[serde(default)]
    unless: Vec<FileCheck>,
    #[serde(default)]
    manual: BTreeMap<String, String>,
    #[serde(default)]
    before_aur: bool,
}

impl From<CommandRepr> for Command {
    fn from(r: CommandRepr) -> Self {
        match r {
            CommandRepr::Argv(argv) => {
                Command { argv, when: vec![], unless: vec![], manual: BTreeMap::new(), before_aur: false }
            }
            CommandRepr::Full(FullCommand { argv, when, unless, manual, before_aur }) => {
                Command { argv, when, unless, manual, before_aur }
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub id: String,
    /// Filled from the enclosing file's `[category]`.
    #[serde(default)]
    pub category: String,
    pub name: String,
    /// Heading the module sits under within its category ("Browsers").
    #[serde(default)]
    pub group: String,
    pub description: String,
    #[serde(default)]
    pub risk: Risk,
    /// Pre-selected in every profile (still subject to `when`).
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub profiles: Vec<String>,
    /// Fact tags that must all hold; `!tag` negates. See `detect::Facts::tags`.
    #[serde(default)]
    pub when: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub packages: Vec<String>,
    /// Installed packages that `packages` conflict with; the module is
    /// skipped while any is installed (pacman --noconfirm would abort).
    #[serde(default)]
    pub package_conflicts: Vec<String>,
    /// Remove `package_conflicts` after the module's AUR or release install
    /// and its root commands instead of before, so a root command can read
    /// their files first (scdr importing ananicy rules). Only for modules
    /// whose install doesn't clash with them.
    #[serde(default)]
    pub replace_after_install: bool,
    #[serde(default)]
    pub aur: Vec<String>,
    /// Pinned GitHub release assets (`[[module.release]]`).
    #[serde(default, rename = "release")]
    pub releases: Vec<ReleaseAsset>,
    #[serde(default)]
    pub enable: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    /// `[options]` keys in pacman.conf; empty value means a bare flag.
    #[serde(default)]
    pub pacman: BTreeMap<String, String>,
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub files: Vec<FileSpec>,
    #[serde(default)]
    pub root_commands: Vec<Command>,
    /// Run as the user; `unless` paths may start with `~/`.
    #[serde(default)]
    pub user_commands: Vec<Command>,
    /// Computed module expanded from detected facts (see `builtins`).
    #[serde(default)]
    pub builtin: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Module {
    /// Installs from the AUR or a release: its root commands, `package_conflicts`
    /// removal and `enable` run after that install (see `plan::resolve`).
    pub fn installs_late(&self) -> bool {
        !self.aur.is_empty() || !self.releases.is_empty()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoryFile {
    category: Category,
    #[serde(default, rename = "module")]
    modules: Vec<Module>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    #[serde(rename = "profile")]
    profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Catalog {
    pub distro: String,
    pub categories: Vec<Category>,
    pub profiles: Vec<Profile>,
    pub modules: Vec<Module>,
}

impl Catalog {
    /// The catalog compiled into the binary for Arch and derivatives.
    pub fn arch() -> Result<Self> {
        Self::from_sources("arch", ARCH_FILES, PROFILES)
    }

    pub fn from_sources(distro: &str, files: &[(&str, &str)], profiles: &str) -> Result<Self> {
        let mut categories = Vec::new();
        let mut modules = Vec::new();
        for (name, text) in files {
            let file: CategoryFile = toml::from_str(text).with_context(|| format!("catalog file {name}.toml"))?;
            for mut module in file.modules {
                module.category = file.category.id.clone();
                modules.push(module);
            }
            categories.push(file.category);
        }
        categories.sort_by_key(|c| c.order);
        let profiles: ProfileFile = toml::from_str(profiles).context("profiles.toml")?;
        let catalog = Self { distro: distro.into(), categories, profiles: profiles.profiles, modules };
        validate::validate(&catalog)?;
        Ok(catalog)
    }

    pub fn module(&self, id: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.id == id)
    }

    pub fn modules_in<'a>(&'a self, category: &'a str) -> impl Iterator<Item = &'a Module> + 'a {
        self.modules.iter().filter(move |m| m.category == category)
    }

    fn category_order(&self, id: &str) -> u32 {
        self.categories.iter().find(|c| c.id == id).map_or(u32::MAX, |c| c.order)
    }

    /// Catalog index of a module, ordered by category then declaration.
    pub fn sort_key(&self, id: &str) -> (u32, usize) {
        let idx = self.modules.iter().position(|m| m.id == id).unwrap_or(usize::MAX);
        let cat = self.module(id).map_or(u32::MAX, |m| self.category_order(&m.category));
        (cat, idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_arch_catalog_is_valid() {
        let c = Catalog::arch().unwrap();
        assert!(c.modules.len() > 20);
        assert!(c.module("zram").is_some());
        assert_eq!(c.categories.first().unwrap().id, "base");
    }

    const PROFILES: &str = "[[profile]]\nid = \"p\"\nname = \"P\"\ndescription = \"\"\n";

    fn load(modules: &str) -> Result<Catalog> {
        let file = format!("[category]\nid = \"x\"\nname = \"X\"\ndescription = \"\"\n{modules}");
        Catalog::from_sources("t", &[("x", &file)], PROFILES)
    }

    #[test]
    fn rejects_unknown_tags_and_one_sided_conflicts() {
        let m = "[[module]]\nid = \"a\"\nname = \"A\"\ndescription = \"\"\n";
        let err = load(&format!("{m}when = [\"labtop\"]")).unwrap_err();
        assert!(err.to_string().contains("unknown `when` tag"));
        assert!(load(&format!("{m}when = [\"!desktop:nope\"]")).is_err());
        assert!(load(&format!("{m}when = [\"!laptop\", \"desktop:plasma\", \"gpu:nvidia\"]")).is_ok());
        let b = "[[module]]\nid = \"b\"\nname = \"B\"\ndescription = \"\"\n";
        let err = load(&format!("{m}conflicts = [\"b\"]\n{b}")).unwrap_err();
        assert!(err.to_string().contains("not the other way round"));
    }

    #[test]
    fn commands_accept_argv_or_guarded_tables() {
        let m = r#"
            [[module]]
            id = "a"
            name = "A"
            description = ""
            root_commands = [["true"], { argv = ["ufw", "enable"], when = ["kdeconnect"], unless = [{ path = "/etc/ufw/ufw.conf", contains = "ENABLED=yes" }] }]
            user_commands = [{ argv = ["x"], unless = [{ path = "~/.x" }] }]
        "#;
        let c = load(m).unwrap();
        let m = c.module("a").unwrap();
        assert_eq!(m.root_commands[0].argv, ["true"]);
        let guarded = &m.root_commands[1];
        assert!(guarded.unless[0].holds(Some("ENABLED=yes\n")) && !guarded.unless[0].holds(Some("ENABLED=no")));
        assert!(!guarded.unless[0].holds(None));
        let bad = r#"
            [[module]]
            id = "a"
            name = "A"
            description = ""
            root_commands = [{ argv = ["x"], unless = [{ path = "~/.x" }] }]
        "#;
        assert!(load(bad).unwrap_err().to_string().contains("non-absolute"));
    }
}
