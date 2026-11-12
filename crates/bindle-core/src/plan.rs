//! Turns a selection into an ordered, idempotent list of steps.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::builtins::{self, Ctx};
use crate::catalog::{AssetKind, Catalog, Command, FileSpec, ReleaseAsset};
use crate::detect::Facts;

#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub profile: Option<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// Remove installed packages that a module's packages conflict with
    /// (`package_conflicts`) instead of skipping the module.
    pub replace_conflicts: bool,
}

impl Selection {
    /// Module ids this selection resolves to before conditions and deps.
    pub fn initial(&self, catalog: &Catalog) -> Result<BTreeSet<String>> {
        if let Some(p) = &self.profile
            && !catalog.profiles.iter().any(|x| &x.id == p)
        {
            bail!("unknown profile `{p}`");
        }
        for id in self.include.iter().chain(&self.exclude) {
            if catalog.module(id).is_none() {
                bail!("unknown module `{id}`");
            }
        }
        let mut set: BTreeSet<String> = catalog
            .modules
            .iter()
            .filter(|m| m.default || self.profile.as_ref().is_some_and(|p| m.profiles.contains(p)))
            .map(|m| m.id.clone())
            .collect();
        set.extend(self.include.iter().cloned());
        set.retain(|id| !self.exclude.contains(id));
        Ok(set)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Step {
    EditPacmanConf {
        options: BTreeMap<String, String>,
        enable_repos: Vec<String>,
    },
    /// Conflicting packages being replaced (`pacman -Rdd`): the next step
    /// installs packages that provide what they did. A module with
    /// `replace_after_install` has its own removal after its AUR or release
    /// install and root commands, which may read the removed packages' files.
    RemovePackages {
        packages: Vec<String>,
    },
    InstallPackages {
        packages: Vec<String>,
    },
    WriteFile {
        module: String,
        file: FileSpec,
    },
    EnableUnits {
        units: Vec<String>,
    },
    /// Units a conflicting module enabled earlier (see `plan::retire`).
    DisableUnits {
        units: Vec<String>,
    },
    /// Unmodified files Bindle wrote for a conflicting module.
    RemoveFiles {
        paths: Vec<String>,
    },
    AddToGroups {
        user: String,
        groups: Vec<String>,
    },
    RunAsRoot {
        module: String,
        argv: Vec<String>,
    },
    RebuildInitramfs,
    BootstrapAurHelper {
        package: String,
    },
    InstallAur {
        packages: Vec<String>,
    },
    /// Pinned GitHub release assets: downloaded and checked as the user, then
    /// packages go through the root helper and AppImages to ~/.local/bin.
    InstallReleases {
        assets: Vec<ReleaseAsset>,
    },
    RunAsUser {
        module: String,
        argv: Vec<String>,
        /// Exit code → what the user still has to do (see `Command::manual`).
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        manual: BTreeMap<String, String>,
    },
}

impl Step {
    pub fn privileged(&self) -> bool {
        !matches!(
            self,
            Step::BootstrapAurHelper { .. }
                | Step::InstallAur { .. }
                | Step::InstallReleases { .. }
                | Step::RunAsUser { .. }
        )
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Plan {
    pub modules: Vec<String>,
    /// (module, reason) pairs for selected modules that will not run.
    pub skipped: Vec<(String, String)>,
    /// (module, required-by) pairs added to satisfy `requires`.
    pub pulled_in: Vec<(String, String)>,
    /// (module, reason) pairs the installer already set up.
    pub provided: Vec<(String, String)>,
    /// (module, installed package) pairs removed to make way for the module.
    pub replaced: Vec<(String, String)>,
    pub notes: Vec<(String, String)>,
    pub steps: Vec<Step>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// `current_file` returns the existing content of a path so identical
/// writes can be skipped; pass `|_| None` to plan from scratch.
pub fn resolve(
    catalog: &Catalog,
    facts: &Facts,
    selection: &Selection,
    current_file: &dyn Fn(&str) -> Option<String>,
) -> Result<Plan> {
    let mut plan = Plan::default();
    let mut chosen = selection.initial(catalog)?;

    // Pull in requirements transitively.
    let mut queue: Vec<String> = chosen.iter().cloned().collect();
    while let Some(id) = queue.pop() {
        for dep in &catalog.module(&id).expect("validated").requires {
            if selection.exclude.contains(dep) {
                bail!("`{id}` requires `{dep}`, which is excluded");
            }
            if chosen.insert(dep.clone()) {
                plan.pulled_in.push((dep.clone(), id.clone()));
                queue.push(dep.clone());
            }
        }
    }

    // Modules the installer already set up are done; their rivals are moot.
    let provided = facts.install.provided();
    for (id, why) in &provided {
        if chosen.remove(id) {
            plan.provided.push((id.clone(), why.clone()));
        }
    }
    plan.pulled_in.retain(|(dep, _)| !provided.contains_key(dep));

    // Drop modules whose conditions fail, then anything depending on them.
    let mut dropped = BTreeSet::new();
    for id in &chosen {
        let m = catalog.module(id).expect("validated");
        if let Some(c) = m.conflicts.iter().find(|c| provided.contains_key(*c)) {
            plan.skipped.push((id.clone(), format!("`{c}` is already set up")));
            dropped.insert(id.clone());
        } else if let Some(p) = m.package_conflicts.iter().find(|p| facts.installed.contains(*p))
            && !selection.replace_conflicts
        {
            plan.skipped.push((id.clone(), format!("conflicts with installed `{p}`; remove it first to use this")));
            dropped.insert(id.clone());
        } else if let Err(reason) = facts.matches(&m.when) {
            plan.skipped.push((id.clone(), reason));
            dropped.insert(id.clone());
        }
    }
    loop {
        let before = dropped.len();
        for id in &chosen {
            let m = catalog.module(id).expect("validated");
            if !dropped.contains(id)
                && let Some(dep) = m.requires.iter().find(|d| dropped.contains(*d))
            {
                plan.skipped.push((id.clone(), format!("requires skipped `{dep}`")));
                dropped.insert(id.clone());
            }
        }
        if dropped.len() == before {
            break;
        }
    }
    chosen.retain(|id| !dropped.contains(id));

    for id in &chosen {
        if let Some(c) = catalog.module(id).expect("validated").conflicts.iter().find(|c| chosen.contains(*c)) {
            bail!("`{id}` conflicts with `{c}`; choose one");
        }
    }

    let mut ordered: Vec<String> = chosen.into_iter().collect();
    ordered.sort_by_key(|id| catalog.sort_key(id));

    let multilib = facts.multilib_enabled
        || ordered.iter().any(|id| catalog.module(id).is_some_and(|m| m.repos.iter().any(|r| r == "multilib")));
    // A module that installs a desktop (uQ) has the `desktop` builtin round out
    // that desktop rather than the one it ships on.
    let planned =
        crate::desktops::all().ok().and_then(|d| crate::desktops::with_planned(&d, &facts.desktops, &ordered));
    let with_planned = planned.map(|desktops| Facts { desktops, ..facts.clone() });
    let ctx = Ctx { facts: with_planned.as_ref().unwrap_or(facts), multilib };

    let mut options = BTreeMap::new();
    let mut repos = Vec::new();
    let mut packages = Vec::new();
    let mut files = Vec::new();
    let mut units = Vec::new();
    let mut groups = Vec::new();
    let mut root_cmds = Vec::new();
    let mut user_cmds = Vec::new();
    // Root steps of modules that install from the AUR or a release: they
    // run after that install, so they can use what it installed.
    let mut late_root_cmds = Vec::new();
    let mut late_units = Vec::new();
    let mut late_removed = Vec::new();
    let mut aur = Vec::new();
    let mut releases: Vec<ReleaseAsset> = Vec::new();
    let mut bootstrap = false;
    let mut initramfs = false;

    for id in &ordered {
        let m = catalog.module(id).expect("validated");
        let late = m.installs_late();
        if selection.replace_conflicts {
            let installed: Vec<String> =
                m.package_conflicts.iter().filter(|p| facts.installed.contains(*p)).cloned().collect();
            if late && m.replace_after_install {
                late_removed.extend(installed.iter().cloned());
            }
            plan.replaced.extend(installed.into_iter().map(|p| (id.clone(), p)));
        }
        options.extend(m.pacman.clone());
        repos.extend(m.repos.iter().filter(|r| !(r.as_str() == "multilib" && facts.multilib_enabled)).cloned());
        packages.extend(m.packages.iter().cloned());
        files.extend(m.files.iter().map(|f| (id.clone(), f.clone())));
        if late { &mut late_units } else { &mut units }.extend(m.enable.iter().cloned());
        groups.extend(m.groups.iter().cloned());
        let pending = |c: &&Command| facts.matches(&c.when).is_ok() && !already_done(c, facts, current_file);
        let cmds = m.root_commands.iter().filter(pending).map(|c| (id.clone(), c.argv.clone()));
        if late { &mut late_root_cmds } else { &mut root_cmds }.extend(cmds);
        user_cmds.extend(m.user_commands.iter().filter(pending).map(|c| (id.clone(), c.clone())));
        aur.extend(m.aur.iter().cloned());
        releases.extend(m.releases.iter().map(|r| ReleaseAsset { title: m.name.clone(), ..r.clone() }));
        plan.notes.extend(m.notes.iter().map(|n| (id.clone(), n.clone())));
        if let Some(b) = &m.builtin {
            let e = builtins::expand(b, &ctx);
            packages.extend(e.packages);
            aur.extend(e.aur);
            files.extend(e.files.into_iter().map(|f| (id.clone(), f)));
            units.extend(e.enable);
            bootstrap |= e.aur_bootstrap;
            initramfs |= e.rebuild_initramfs;
            plan.notes.extend(e.notes.into_iter().map(|n| (id.clone(), n)));
        }
    }

    let wanted_units = units.iter().chain(&late_units).map(String::as_str).collect();
    let wanted_files = files.iter().map(|(_, f)| f.path.as_str()).collect();
    let retired = retire::rivals(catalog, &ordered, facts, current_file, &wanted_units, &wanted_files);
    plan.notes.extend(retired.notes);

    // Keep only pacman.conf edits that would change something.
    if let Some(current) = current_file("/etc/pacman.conf") {
        options.retain(|k, v| crate::pacman_conf::set_option(&current, k, v) != current);
        repos.retain(|r| !crate::pacman_conf::repo_enabled(&current, r));
    }
    let repos = dedup(repos);
    if !options.is_empty() || !repos.is_empty() {
        plan.steps.push(Step::EditPacmanConf { options, enable_repos: repos });
    }
    let late_removed = dedup(late_removed);
    let removed = dedup(plan.replaced.iter().map(|(_, p)| p.clone()).filter(|p| !late_removed.contains(p)));
    if !removed.is_empty() {
        plan.steps.push(Step::RemovePackages { packages: removed });
    }
    let packages = dedup(packages.into_iter().filter(|p| !facts.has_package(p)));
    if !packages.is_empty() {
        plan.steps.push(Step::InstallPackages { packages });
    }
    for (module, file) in files {
        if current_file(&file.path).as_deref() != Some(file.content.as_str()) {
            plan.steps.push(Step::WriteFile { module, file });
        }
    }
    // A replaced rival goes once its replacement's packages and files are in.
    plan.steps.extend(retired.steps);
    let enabled = |u: &String| !facts.enabled_units.contains(u);
    let late_units = dedup(late_units.into_iter().filter(enabled));
    let units = dedup(units.into_iter().filter(enabled).filter(|u| !late_units.contains(u)));
    if !units.is_empty() {
        plan.steps.push(Step::EnableUnits { units });
    }
    let groups = dedup(groups.into_iter().filter(|g| !facts.user_groups.contains(g)));
    if !groups.is_empty() {
        plan.steps.push(Step::AddToGroups { user: facts.user.clone(), groups });
    }
    plan.steps.extend(root_cmds.into_iter().map(|(module, argv)| Step::RunAsRoot { module, argv }));
    if initramfs {
        plan.steps.push(Step::RebuildInitramfs);
    }
    // User commands the AUR builds need (rustup's default toolchain: without
    // it every Rust PKGBUILD fails with "no default is configured").
    let (early_cmds, user_cmds): (Vec<_>, Vec<_>) = user_cmds.into_iter().partition(|(_, c)| c.before_aur);
    plan.steps.extend(early_cmds.into_iter().map(|(module, c)| Step::RunAsUser {
        module,
        argv: c.argv,
        manual: c.manual,
    }));
    if bootstrap {
        plan.steps.push(Step::BootstrapAurHelper { package: builtins::AUR_HELPER.into() });
    }
    let aur = dedup(aur.into_iter().filter(|p| !facts.has_package(p)));
    if !aur.is_empty() {
        plan.steps.push(Step::InstallAur { packages: aur });
    }
    releases.retain(|r| !release_installed(r, facts, current_file));
    if !releases.is_empty() {
        plan.steps.push(Step::InstallReleases { assets: releases });
    }
    // After the AUR and releases: the late modules' root commands (an import
    // that reads what a conflicting package left), then the removal of those
    // packages, then their units.
    plan.steps.extend(late_root_cmds.into_iter().map(|(module, argv)| Step::RunAsRoot { module, argv }));
    if !late_removed.is_empty() {
        plan.steps.push(Step::RemovePackages { packages: late_removed });
    }
    if !late_units.is_empty() {
        plan.steps.push(Step::EnableUnits { units: late_units });
    }
    plan.steps.extend(user_cmds.into_iter().map(|(module, c)| Step::RunAsUser {
        module,
        argv: c.argv,
        manual: c.manual,
    }));

    let advisories = crate::install::advisories(facts);
    plan.notes.extend(advisories.into_iter().map(|a| ("install".to_string(), a)));
    plan.modules = ordered;
    Ok(plan)
}

/// Every `unless` check holds, so the command's effect is already in place.
/// `~/` paths resolve against the user's home (unknown home: not done).
fn already_done(c: &Command, facts: &Facts, current_file: &dyn Fn(&str) -> Option<String>) -> bool {
    !c.unless.is_empty()
        && c.unless.iter().all(|u| {
            let path = match u.path.strip_prefix("~/") {
                Some(_) if facts.home.is_empty() => return false,
                Some(rest) => format!("{}/{rest}", facts.home.trim_end_matches('/')),
                None => u.path.clone(),
            };
            u.holds(current_file(&path).as_deref())
        })
}

/// A package by name; an AppImage by its desktop entry naming this asset.
fn release_installed(r: &ReleaseAsset, facts: &Facts, current_file: &dyn Fn(&str) -> Option<String>) -> bool {
    match r.kind() {
        Some(AssetKind::Package) => facts.installed.contains(&r.name),
        Some(AssetKind::AppImage) if !facts.home.is_empty() => {
            let entry = r.desktop_entry();
            let path = format!("{}/{}", facts.home.trim_end_matches('/'), entry.trim_start_matches("~/"));
            current_file(&path).is_some_and(|c| c.contains(&format!("X-Bindle-Asset={}", r.asset)))
        }
        _ => false,
    }
}

fn dedup(items: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    items.into_iter().filter(|i| seen.insert(i.clone())).collect()
}

pub mod retire;
#[cfg(test)]
mod tests;
