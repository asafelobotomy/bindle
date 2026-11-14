//! Collecting what the ordered modules ask for, before it becomes steps.

use std::collections::{BTreeMap, BTreeSet};

use super::{Plan, Selection, content};
use crate::builtins::{self, Ctx};
use crate::catalog::{Catalog, Command, FileSpec, ReleaseAsset};
use crate::detect::Facts;
use crate::fstab::Entry;

use super::checks::already_done;

/// Everything the modules want, in module order. Packages carry the module
/// that asked for them.
#[derive(Debug, Default)]
pub(super) struct Parts {
    pub options: BTreeMap<String, String>,
    pub repos: Vec<String>,
    pub packages: Vec<(String, String)>,
    pub files: Vec<(String, FileSpec)>,
    pub removed_files: Vec<String>,
    pub units: Vec<String>,
    pub groups: Vec<String>,
    pub root_cmds: Vec<(String, Vec<String>)>,
    pub user_cmds: Vec<(String, Command)>,
    /// Root steps of modules that install from the AUR or a release: they
    /// run after that install, so they can use what it installed.
    pub late_root_cmds: Vec<(String, Vec<String>)>,
    pub late_units: Vec<String>,
    pub late_removed: Vec<String>,
    pub aur: Vec<(String, String)>,
    pub releases: Vec<ReleaseAsset>,
    pub mounts: Vec<Entry>,
    pub bootstrap: bool,
    pub initramfs: bool,
}

/// Collect `ordered`'s parts; also fills `plan`'s replaced, notes and contents.
pub(super) fn collect(
    catalog: &Catalog,
    facts: &Facts,
    selection: &Selection,
    ordered: &[String],
    ctx: &Ctx,
    current_file: &dyn Fn(&str) -> Option<String>,
    plan: &mut Plan,
) -> Parts {
    let mut p = Parts::default();
    for id in ordered {
        let m = catalog.module(id).expect("validated");
        let late = m.installs_late();
        if selection.replace_conflicts {
            let installed: Vec<String> =
                m.package_conflicts.iter().filter(|p| facts.installed.contains(*p)).cloned().collect();
            if late && m.replace_after_install {
                p.late_removed.extend(installed.iter().cloned());
            }
            plan.replaced.extend(installed.into_iter().map(|p| (id.clone(), p)));
        }
        p.options.extend(m.pacman.clone());
        p.repos.extend(m.repos.iter().filter(|r| !(r.as_str() == "multilib" && facts.multilib_enabled)).cloned());
        p.packages.extend(m.packages.iter().map(|x| (id.clone(), x.clone())));
        p.files.extend(m.files.iter().map(|f| (id.clone(), f.clone())));
        if late { &mut p.late_units } else { &mut p.units }.extend(m.enable.iter().cloned());
        p.groups.extend(m.groups.iter().cloned());
        let pending = |c: &&Command| facts.matches(&c.when).is_ok() && !already_done(c, facts, current_file);
        let cmds = m.root_commands.iter().filter(pending).map(|c| (id.clone(), c.argv.clone()));
        if late { &mut p.late_root_cmds } else { &mut p.root_cmds }.extend(cmds);
        p.user_cmds.extend(m.user_commands.iter().filter(pending).map(|c| (id.clone(), c.clone())));
        p.aur.extend(m.aur.iter().map(|x| (id.clone(), x.clone())));
        p.releases.extend(m.releases.iter().map(|r| ReleaseAsset { title: m.name.clone(), ..r.clone() }));
        plan.notes.extend(m.notes.iter().map(|n| (id.clone(), n.clone())));
        let expansion = m.builtin.as_ref().map(|b| builtins::expand(b, ctx));
        plan.contents.insert(id.clone(), content::of(m, expansion.as_ref(), ctx.facts));
        if let Some(e) = expansion {
            p.packages.extend(e.packages.into_iter().map(|x| (id.clone(), x)));
            p.aur.extend(e.aur.into_iter().map(|x| (id.clone(), x)));
            p.files.extend(e.files.into_iter().map(|f| (id.clone(), f)));
            p.removed_files.extend(e.remove_files);
            p.units.extend(e.enable);
            p.mounts.extend(e.mounts);
            p.bootstrap |= e.aur_bootstrap;
            p.initramfs |= e.rebuild_initramfs;
            plan.notes.extend(e.notes.into_iter().map(|n| (id.clone(), n)));
        }
    }
    p
}

/// A chosen module's `package_conflicts` are not installed by any other
/// module of the plan: noti-fi replaces mako, which the desktop essentials
/// would otherwise bring. Each package that would have been installed gets
/// a note on the module that keeps it out.
pub(super) fn drop_package_conflicts(
    catalog: &Catalog,
    facts: &Facts,
    ordered: &[String],
    parts: &mut Parts,
    plan: &mut Plan,
) {
    let mut barred: BTreeMap<&str, &str> = BTreeMap::new();
    for id in ordered {
        for p in &catalog.module(id).expect("validated").package_conflicts {
            barred.entry(p.as_str()).or_insert(id.as_str());
        }
    }
    if barred.is_empty() {
        return;
    }
    let mut noted = BTreeSet::new();
    for list in [&mut parts.packages, &mut parts.aur] {
        list.retain(|(from, pkg)| {
            let Some(by) = barred.get(pkg.as_str()) else { return true };
            if !facts.has_package(pkg) && noted.insert(pkg.clone()) {
                plan.notes.push((by.to_string(), format!("not installing `{pkg}` for `{from}`: `{by}` replaces it")));
            }
            false
        });
    }
}
