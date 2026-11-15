//! Reading the catalog's TOML: one file per category, or a file that adds
//! modules to a category another file defines (`extends`).

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{Catalog, Category, Module, Profile, validate};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoryFile {
    /// A file adds modules to a category another file defines (`extends`)
    /// or defines its own.
    category: Option<Category>,
    extends: Option<String>,
    #[serde(default, rename = "module")]
    modules: Vec<Module>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    #[serde(rename = "profile")]
    profiles: Vec<Profile>,
}

pub(super) fn from_sources(distro: &str, files: &[(&str, &str)], profiles: &str) -> Result<Catalog> {
    let mut categories = Vec::new();
    let mut modules = Vec::new();
    let mut extends = Vec::new();
    for (name, text) in files {
        let file: CategoryFile = toml::from_str(text).with_context(|| format!("catalog file {name}.toml"))?;
        let id = match (file.category, file.extends) {
            (Some(c), None) => {
                let id = c.id.clone();
                categories.push(c);
                id
            }
            (None, Some(id)) => {
                extends.push((name.to_string(), id.clone()));
                id
            }
            _ => anyhow::bail!("catalog file {name}.toml: give either [category] or `extends`"),
        };
        for mut module in file.modules {
            module.category = id.clone();
            modules.push(module);
        }
    }
    if let Some((name, id)) = extends.iter().find(|(_, id)| !categories.iter().any(|c| &c.id == id)) {
        anyhow::bail!("catalog file {name}.toml extends unknown category {id}");
    }
    categories.sort_by_key(|c| c.order);
    let profiles: ProfileFile = toml::from_str(profiles).context("profiles.toml")?;
    let catalog = Catalog { distro: distro.into(), categories, profiles: profiles.profiles, modules };
    validate::validate(&catalog)?;
    Ok(catalog)
}
