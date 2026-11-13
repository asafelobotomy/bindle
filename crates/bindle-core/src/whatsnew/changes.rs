//! `catalog/changes.toml`: what a diff of the catalog can't tell "What's
//! new": renamed module ids, modules that left the catalog, and optional
//! modules worth showing to people upgrading to a version.

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::catalog::Catalog;

const CHANGES: &str = include_str!("../../../../catalog/changes.toml");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Changes {
    #[serde(default)]
    pub renamed: Vec<Renamed>,
    #[serde(default)]
    pub retired: Vec<Retired>,
    #[serde(default)]
    pub added: Vec<Added>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Renamed {
    pub from: String,
    pub to: String,
    pub version: String,
    pub why: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retired {
    pub id: String,
    pub version: String,
    pub why: String,
    #[serde(default)]
    pub replaced_by: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Added {
    pub version: String,
    pub modules: Vec<String>,
    pub why: String,
}

fn version_like(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

impl Changes {
    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// The built-in table, checked against `catalog`.
    pub fn arch(catalog: &Catalog) -> Result<Self> {
        let c = Self::parse(CHANGES)?;
        c.validate(catalog)?;
        Ok(c)
    }

    pub fn validate(&self, catalog: &Catalog) -> Result<()> {
        let known = |id: &str| catalog.module(id).is_some();
        for r in &self.renamed {
            if known(&r.from) || !known(&r.to) || !version_like(&r.version) {
                bail!("renamed `{}` → `{}`: the old id must be gone, the new one in the catalog", r.from, r.to);
            }
        }
        for r in &self.retired {
            if known(&r.id) || !version_like(&r.version) || r.why.trim().is_empty() {
                bail!("retired `{}`: must have left the catalog, with a version and a why", r.id);
            }
            if let Some(by) = r.replaced_by.as_deref().filter(|by| !known(by)) {
                bail!("retired `{}` is replaced by unknown `{by}`", r.id);
            }
        }
        for a in &self.added {
            if !version_like(&a.version) || a.why.trim().is_empty() {
                bail!("added in `{}`: needs a version like 1.2.3 and a why", a.version);
            }
            if let Some(m) = a.modules.iter().find(|m| !known(m)) {
                bail!("added in {}: unknown module `{m}`", a.version);
            }
        }
        Ok(())
    }

    /// What was applied under an old id now counts as the new one.
    pub fn current_id<'a>(&'a self, id: &'a str) -> &'a str {
        self.renamed.iter().find(|r| r.from == id).map_or(id, |r| r.to.as_str())
    }
}
