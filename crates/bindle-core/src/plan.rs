//! Turns a selection into an ordered, idempotent list of steps.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::builtins::Ctx;
use crate::catalog::{Catalog, FileSpec, ReleaseAsset};
use crate::detect::Facts;

#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub profile: Option<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// Remove installed packages that a module's packages conflict with
    /// (`package_conflicts`) instead of skipping the module.
    pub replace_conflicts: bool,
    /// Exactly `include` (and what it requires): no profile, no defaults.
    /// For applying only what the user picked on "What's new".
    pub exact: bool,
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
            if catalog.module(id).is_none() && !mounts::absent(catalog, id) {
                bail!("unknown module `{id}`");
            }
        }
        let mut set: BTreeSet<String> = catalog
            .modules
            .iter()
            .filter(|_| !self.exact)
            .filter(|m| m.default || self.profile.as_ref().is_some_and(|p| m.profiles.contains(p)))
            .map(|m| m.id.clone())
            .collect();
        set.extend(self.include.iter().filter(|id| catalog.module(id).is_some()).cloned());
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
    /// Add drives to Bindle's block in /etc/fstab and create their mount points.
    MountAtBoot {
        entries: Vec<crate::fstab::Entry>,
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
    /// What each module puts on this system, resolved (recorded at apply).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub contents: BTreeMap<String, content::Content>,
    /// Modules this plan retires (What's new clean-ups): their record is dropped.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub forget: Vec<String>,
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
    plan.skipped.extend(mounts::skipped(catalog, selection));
    select::pull_in_requires(catalog, selection, &mut chosen, &mut plan)?;
    // Modules the installer already set up are done; their rivals are moot.
    let provided = select::drop_provided(facts, &mut chosen, &mut plan);
    select::drop_unmet(catalog, facts, selection, &provided, &mut chosen, &mut plan);
    select::check_conflicts(catalog, &chosen)?;

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

    let mut parts = gather::collect(catalog, facts, selection, &ordered, &ctx, current_file, &mut plan);
    gather::drop_package_conflicts(catalog, facts, &ordered, &mut parts, &mut plan);
    emit::steps(catalog, facts, &ordered, parts, current_file, &mut plan);

    let advisories = crate::install::advisories(facts);
    plan.notes.extend(advisories.into_iter().map(|a| ("install".to_string(), a)));
    plan.modules = ordered;
    Ok(plan)
}

mod checks;
#[cfg(test)]
mod conflict_tests;
pub mod content;
mod emit;
mod gather;
mod mounts;
pub mod retire;
mod select;
#[cfg(test)]
mod tests;
