//! The last phase of `resolve`: gathered parts become ordered steps, leaving
//! out what is already in place and retiring conflicting modules' leftovers.

use super::checks::{dedup, release_installed};
use super::gather::Parts;
use super::{Plan, Step, mounts, retire};
use crate::builtins;
use crate::catalog::Catalog;
use crate::detect::Facts;

pub(super) fn steps(
    catalog: &Catalog,
    facts: &Facts,
    ordered: &[String],
    parts: Parts,
    current_file: &dyn Fn(&str) -> Option<String>,
    plan: &mut Plan,
) {
    let Parts {
        mut options,
        mut repos,
        packages,
        files,
        removed_files,
        units,
        groups,
        root_cmds,
        user_cmds,
        late_root_cmds,
        late_units,
        late_removed,
        aur,
        mut releases,
        mounts,
        bootstrap,
        initramfs,
    } = parts;

    let wanted_units = units.iter().chain(&late_units).map(String::as_str).collect();
    let wanted_files = files.iter().map(|(_, f)| f.path.as_str()).collect();
    let retired = retire::rivals(catalog, ordered, facts, current_file, &wanted_units, &wanted_files);
    // A file one module drops goes only if no other module writes it, and
    // only while it is still Bindle's.
    let ours = |p: &String| current_file(p).is_some_and(|t| t.starts_with(retire::MARKER));
    let removed_files = dedup(removed_files.into_iter().filter(|p| !wanted_files.contains(p.as_str())).filter(ours));
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
    let packages = dedup(packages.into_iter().map(|(_, p)| p).filter(|p| !facts.has_package(p)));
    if !packages.is_empty() {
        plan.steps.push(Step::InstallPackages { packages });
    }
    for (module, file) in files {
        if current_file(&file.path).as_deref() != Some(file.content.as_str()) {
            plan.steps.push(Step::WriteFile { module, file });
        }
    }
    if !removed_files.is_empty() {
        plan.steps.push(Step::RemoveFiles { paths: removed_files });
    }
    plan.steps.extend(mounts::step(mounts, current_file));
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
    let aur = dedup(aur.into_iter().map(|(_, p)| p).filter(|p| !facts.has_package(p)));
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
}
