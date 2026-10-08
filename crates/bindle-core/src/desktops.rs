//! Desktops offered by archinstall, and what Bindle adds on top of each.

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const DESKTOPS: &str = include_str!("../../../catalog/desktops.toml");

/// Packages archinstall's "Desktop" profile adds before any desktop.
pub const DESKTOP_BASE: &[&str] = &["nano", "vim", "openssh", "htop", "wget", "smartmontools", "xdg-utils"];

/// archinstall greeter value → package that provides it.
pub const GREETERS: &[(&str, &str)] = &[
    ("gdm", "gdm"),
    ("sddm", "sddm"),
    ("lightdm-gtk-greeter", "lightdm-gtk-greeter"),
    ("lightdm-slick-greeter", "lightdm-slick-greeter"),
    ("ly", "ly"),
    ("cosmic-greeter", "cosmic-greeter"),
    ("plasma-login-manager", "plasma-login-manager"),
    ("dms-greeter", "dms-greeter"),
];

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Desktop {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub display: String,
    pub toolkit: String,
    pub detect: String,
    pub archinstall: Vec<String>,
    #[serde(default)]
    pub greeter: Option<String>,
    #[serde(default)]
    pub essentials: Vec<String>,
    #[serde(default)]
    pub nm: Vec<String>,
    #[serde(default)]
    pub bt: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    /// Desktops this one ships as a component (Budgie runs on labwc), so
    /// their packages being installed doesn't mean the user chose them.
    #[serde(default)]
    pub includes: Vec<String>,
    /// Set when archinstall's profile for this desktop is known not to install.
    #[serde(default)]
    pub broken: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DesktopFile {
    desktop: Vec<Desktop>,
}

pub fn all() -> Result<Vec<Desktop>> {
    let file: DesktopFile = toml::from_str(DESKTOPS).context("desktops.toml")?;
    Ok(file.desktop)
}

pub fn by_id<'a>(desktops: &'a [Desktop], id: &str) -> Option<&'a Desktop> {
    desktops.iter().find(|d| d.id == id)
}

/// archinstall profile name (e.g. "KDE Plasma") → desktop.
pub fn by_archinstall_name<'a>(desktops: &'a [Desktop], name: &str) -> Option<&'a Desktop> {
    desktops.iter().find(|d| d.name.eq_ignore_ascii_case(name))
}

/// Desktops on this system: the installer's record plus any detected by their
/// signature package, minus ones that are only a component of another.
pub fn present(desktops: &[Desktop], recorded: &[String], installed: &BTreeSet<String>) -> Vec<String> {
    let mut found: Vec<String> = recorded.to_vec();
    for d in desktops.iter().filter(|d| installed.contains(&d.detect)) {
        if !found.contains(&d.id) {
            found.push(d.id.clone());
        }
    }
    let included: BTreeSet<&String> =
        found.iter().filter_map(|id| by_id(desktops, id)).flat_map(|d| d.includes.iter()).collect();
    found.retain(|id| !included.contains(id));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_archinstall_desktops_are_known() {
        let d = all().unwrap();
        assert_eq!(d.len(), 21);
        assert_eq!(by_archinstall_name(&d, "KDE Plasma").unwrap().id, "plasma");
        assert!(d.iter().all(|x| ["wayland", "xorg"].contains(&x.display.as_str())));
        for x in &d {
            for inc in &x.includes {
                assert!(by_id(&d, inc).is_some(), "{} includes unknown {inc}", x.id);
            }
        }
    }

    #[test]
    fn components_are_not_separate_desktops() {
        let d = all().unwrap();
        let installed: BTreeSet<String> = ["budgie-desktop", "labwc"].map(String::from).into();
        assert_eq!(present(&d, &["budgie".into()], &installed), ["budgie"]);
        // labwc on its own is still a desktop.
        let installed: BTreeSet<String> = ["labwc".to_string()].into();
        assert_eq!(present(&d, &[], &installed), ["labwc"]);
    }
}
