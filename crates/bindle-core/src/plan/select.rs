//! The first phases of `resolve`: which modules the plan runs.
//!
//! Requirements are pulled in, modules the installer provided are dropped,
//! then modules whose conditions fail (and anything needing them), and what
//! remains must not conflict.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use super::{Plan, Selection};
use crate::catalog::Catalog;
use crate::detect::Facts;

/// Add every module `chosen` requires, transitively.
pub(super) fn pull_in_requires(
    catalog: &Catalog,
    selection: &Selection,
    chosen: &mut BTreeSet<String>,
    plan: &mut Plan,
) -> Result<()> {
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
    Ok(())
}

/// Modules the installer already set up are done; returns them (id → why).
pub(super) fn drop_provided(facts: &Facts, chosen: &mut BTreeSet<String>, plan: &mut Plan) -> BTreeMap<String, String> {
    let provided = facts.install.provided();
    for (id, why) in &provided {
        if chosen.remove(id) {
            plan.provided.push((id.clone(), why.clone()));
        }
    }
    plan.pulled_in.retain(|(dep, _)| !provided.contains_key(dep));
    provided
}

/// Drop modules whose conditions fail, then anything depending on them.
pub(super) fn drop_unmet(
    catalog: &Catalog,
    facts: &Facts,
    selection: &Selection,
    provided: &BTreeMap<String, String>,
    chosen: &mut BTreeSet<String>,
    plan: &mut Plan,
) {
    let mut dropped = BTreeSet::new();
    for id in chosen.iter() {
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
        for id in chosen.iter() {
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
}

/// Two chosen modules that conflict are the user's call, not ours.
pub(super) fn check_conflicts(catalog: &Catalog, chosen: &BTreeSet<String>) -> Result<()> {
    for id in chosen {
        if let Some(c) = catalog.module(id).expect("validated").conflicts.iter().find(|c| chosen.contains(*c)) {
            bail!("`{id}` conflicts with `{c}`; choose one");
        }
    }
    Ok(())
}
