//! Load-time checks: a broken catalog fails `cargo test`, never a user's apply.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use super::{Catalog, Module};
use crate::{builtins, desktops, detect};

const TOOLKITS: &[&str] = &["qt", "gtk", "other"];
const DISPLAYS: &[&str] = &["wayland", "xorg"];

pub(super) fn validate(cat: &Catalog) -> Result<()> {
    unique("category", cat.categories.iter().map(|c| c.id.as_str()))?;
    unique("profile", cat.profiles.iter().map(|p| p.id.as_str()))?;
    unique("module", cat.modules.iter().map(|m| m.id.as_str()))?;
    let ids: BTreeSet<&str> = cat.modules.iter().map(|m| m.id.as_str()).collect();
    let profile_ids: BTreeSet<_> = cat.profiles.iter().map(|p| p.id.as_str()).collect();
    let desktops = desktops::all()?;
    desktops::check(&desktops)?;
    for d in &desktops {
        if !TOOLKITS.contains(&d.toolkit.as_str()) || !DISPLAYS.contains(&d.display.as_str()) {
            bail!("desktop `{}` has toolkit `{}` / display `{}`", d.id, d.toolkit, d.display);
        }
    }
    let desktop_ids: BTreeSet<String> = desktops.into_iter().map(|d| d.id).collect();
    let mut writers: BTreeMap<&str, &str> = BTreeMap::new();
    for m in &cat.modules {
        if m.id.is_empty() {
            bail!("a module has an empty id");
        }
        for dep in m.requires.iter().chain(&m.conflicts) {
            if !ids.contains(dep.as_str()) {
                bail!("module `{}` references unknown module `{dep}`", m.id);
            }
        }
        if m.conflicts.contains(&m.id) {
            bail!("module `{}` conflicts with itself", m.id);
        }
        if let Some(p) = m.profiles.iter().find(|p| !profile_ids.contains(p.as_str())) {
            bail!("module `{}` references unknown profile `{p}`", m.id);
        }
        if let Some(b) = &m.builtin
            && !builtins::KNOWN.contains(&b.as_str())
        {
            bail!("module `{}` uses unknown builtin `{b}`", m.id);
        }
        check_names(m)?;
        check_releases(m)?;
        check_files(m, &mut writers)?;
        if let Some(p) = m.package_conflicts.iter().find(|p| p.is_empty() || m.packages.contains(p)) {
            bail!("module `{}` lists `{p}` in package_conflicts", m.id);
        }
        if !m.aur.is_empty() && !m.requires.iter().any(|r| r == "aur-helper") {
            bail!("module `{}` installs from the AUR but doesn't require `aur-helper`", m.id);
        }
        if let Some((k, v)) = m
            .pacman
            .iter()
            .find(|(k, v)| !k.chars().all(|c| c.is_ascii_alphanumeric()) || v.contains(['\n', '\r', '[', ']']))
        {
            bail!("module `{}` sets pacman option `{k}` = `{v}`", m.id);
        }
        if m.root_commands.iter().chain(&m.user_commands).any(|c| c.argv.is_empty()) {
            bail!("module `{}` has an empty command", m.id);
        }
        for c in &m.conflicts {
            let other = cat.module(c).expect("checked above");
            if !other.conflicts.contains(&m.id) {
                bail!("module `{}` conflicts with `{c}`, but not the other way round", m.id);
            }
            if m.default && other.default {
                bail!("default modules `{}` and `{c}` conflict", m.id);
            }
        }
        let commands = m.root_commands.iter().chain(&m.user_commands);
        for tag in m.when.iter().chain(commands.clone().flat_map(|c| &c.when)) {
            let bare = tag.strip_prefix('!').unwrap_or(tag);
            let desktop_ok = bare.strip_prefix("desktop:").is_none_or(|id| desktop_ids.contains(id));
            if !detect::known_tag(bare) || !desktop_ok {
                bail!("module `{}` uses unknown `when` tag `{tag}`", m.id);
            }
        }
        let root_paths = m.root_commands.iter().flat_map(|c| &c.unless).map(|u| (u, false));
        let user_paths = m.user_commands.iter().flat_map(|c| &c.unless).map(|u| (u, true));
        for (u, user) in root_paths.chain(user_paths) {
            if !(u.path.starts_with('/') || user && u.path.starts_with("~/")) {
                bail!("module `{}` checks non-absolute path `{}`", m.id, u.path);
            }
        }
    }
    Ok(())
}

fn unique<'a>(what: &str, ids: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            bail!("duplicate {what} id `{id}`");
        }
    }
    Ok(())
}

/// Package, unit and group names: non-empty, never an option (`-…`), and a
/// package is installed from the repos or the AUR, not both.
fn check_names(m: &Module) -> Result<()> {
    let names = m.packages.iter().chain(&m.aur).chain(&m.package_conflicts).chain(&m.enable).chain(&m.groups);
    if let Some(n) = names.into_iter().find(|n| n.is_empty() || n.starts_with('-') || n.contains(char::is_whitespace)) {
        bail!("module `{}` has an invalid name `{n}`", m.id);
    }
    if let Some(p) = m.packages.iter().find(|p| m.aur.contains(p)) {
        bail!("module `{}` lists `{p}` in both packages and aur", m.id);
    }
    Ok(())
}

/// Pinned, checksummed, and a kind Bindle knows how to install.
fn check_releases(m: &Module) -> Result<()> {
    let word = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    for r in &m.releases {
        let repo_ok = r.repo.split_once('/').is_some_and(|(o, n)| word(o) && word(n) && !n.contains('/'));
        let ok = repo_ok
            && word(&r.tag)
            && word(&r.asset)
            && !r.asset.starts_with('.')
            && r.sha256.len() == 64
            && r.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            && word(&r.name)
            && !r.name.starts_with('-')
            && r.kind().is_some();
        if !ok {
            bail!("module `{}` has an invalid release asset `{}` from `{}`", m.id, r.asset, r.repo);
        }
    }
    Ok(())
}

/// Absolute, no `..`, no setuid/setgid/sticky, one writer per path.
fn check_files<'a>(m: &'a Module, writers: &mut BTreeMap<&'a str, &'a str>) -> Result<()> {
    for f in &m.files {
        if !f.path.starts_with('/') || f.path.split('/').any(|c| c == "..") {
            bail!("module `{}` writes bad path `{}`", m.id, f.path);
        }
        if f.mode > 0o777 {
            bail!("module `{}` writes {} with mode {:o}", m.id, f.path, f.mode);
        }
        if let Some(other) = writers.insert(&f.path, &m.id) {
            bail!("modules `{other}` and `{}` both write {}", m.id, f.path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Catalog;

    const PROFILES: &str = "[[profile]]\nid = \"p\"\nname = \"P\"\ndescription = \"\"\n";

    fn err(modules: &str) -> String {
        let file = format!("[category]\nid = \"x\"\nname = \"X\"\ndescription = \"\"\n{modules}");
        Catalog::from_sources("t", &[("x", &file)], PROFILES).map(drop).unwrap_err().to_string()
    }

    /// `[[module.release]]` with repo, asset and sha256 filled in by `format!`.
    macro_rules! release_toml {
        () => {
            "[[module.release]]\nrepo = \"{}\"\ntag = \"v1\"\nasset = \"{}\"\nsha256 = \"{}\"\nname = \"x\""
        };
    }

    fn module(id: &str, extra: &str) -> String {
        format!("[[module]]\nid = \"{id}\"\nname = \"N\"\ndescription = \"\"\n{extra}\n")
    }

    #[test]
    fn rejects_each_kind_of_broken_module() {
        let file =
            |path: &str, mode: &str| format!("[[module.files]]\npath = \"{path}\"\ncontent = \"\"\nmode = {mode}\n");
        let cases = [
            (module("a", "conflicts = [\"a\"]"), "conflicts with itself"),
            (module("a", "") + &module("a", ""), "duplicate module id"),
            (module("", ""), "empty id"),
            (module("a", "packages = [\"--root=/\"]"), "invalid name"),
            (module("a", "packages = [\"\"]"), "invalid name"),
            (module("a", "enable = [\"x y\"]"), "invalid name"),
            (module("a", "pacman = { Color = \"x\\n[evil]\" }"), "pacman option"),
            (module("a", &file("/etc/../root/x", "0o644")), "bad path"),
            (module("a", &file("/usr/bin/x", "0o4755")), "mode"),
            (module("a", &file("/etc/x", "0o644")) + &module("b", &file("/etc/x", "0o644")), "both write"),
            (module("a", "aur = [\"x-bin\"]"), "aur-helper"),
            (
                module(
                    "a",
                    &format!(
                        release_toml!(),
                        "o/r", "x.exe", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    ),
                ),
                "release asset",
            ),
            (
                module(
                    "a",
                    &format!(
                        release_toml!(),
                        "o/r/../x", "x.AppImage", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    ),
                ),
                "release asset",
            ),
            (module("a", &format!(release_toml!(), "o/r", "x.AppImage", "abc")), "release asset"),
            (
                module("a", "packages = [\"x\"]\naur = [\"x\"]\nrequires = [\"h\"]") + &module("h", ""),
                "both packages and aur",
            ),
            (
                module("a", "default = true\nconflicts = [\"b\"]")
                    + &module("b", "default = true\nconflicts = [\"a\"]"),
                "default modules",
            ),
        ];
        for (toml, want) in cases {
            let e = err(&toml);
            assert!(e.contains(want), "expected `{want}`, got `{e}` for:\n{toml}");
        }
    }

    #[test]
    fn rejects_duplicate_profiles_and_categories() {
        let profiles = format!("{PROFILES}{PROFILES}");
        let file = "[category]\nid = \"x\"\nname = \"X\"\ndescription = \"\"\n";
        let e = Catalog::from_sources("t", &[("x", file)], &profiles).map(drop).unwrap_err();
        assert!(e.to_string().contains("duplicate profile id"));
        let e = Catalog::from_sources("t", &[("x", file), ("y", file)], PROFILES).map(drop).unwrap_err();
        assert!(e.to_string().contains("duplicate category id"));
    }
}
