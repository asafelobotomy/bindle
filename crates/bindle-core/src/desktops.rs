//! Desktops offered by archinstall, and what Bindle adds on top of each.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
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
    /// Empty for a desktop with a `base`: archinstall has no profile for it.
    #[serde(default)]
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
    /// Desktop whose archinstall profile `bindle iso` installs for this one
    /// (uQ installs Hyprland's); `module` finishes it on first login.
    #[serde(default)]
    pub base: Option<String>,
    /// Catalog module that installs this desktop. While it is planned, the
    /// `desktop` builtin rounds out this desktop instead of the ones it `includes`.
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub status: Option<Status>,
}

/// How Bindle offers a desktop. Only one desktop may be `recommended`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Offered, labelled "(preview)", never pre-selected.
    Preview,
    /// Labelled "(recommended)" and pre-selected in `bindle iso`.
    Recommended,
}

impl Desktop {
    /// The name as the pickers show it: "uQ (preview)", "uQ (recommended)".
    pub fn label(&self) -> String {
        match self.status {
            Some(Status::Preview) => format!("{} (preview)", self.name),
            Some(Status::Recommended) => format!("{} (recommended)", self.name),
            None => self.name.clone(),
        }
    }

    /// The desktop whose archinstall profile installs this one: its `base`, or itself.
    pub fn profile<'a>(&'a self, desktops: &'a [Desktop]) -> &'a Desktop {
        self.base.as_deref().and_then(|b| by_id(desktops, b)).unwrap_or(self)
    }
}

#[derive(Debug, Deserialize)]
struct DesktopFile {
    desktop: Vec<Desktop>,
}

pub fn all() -> Result<Vec<Desktop>> {
    let file: DesktopFile = toml::from_str(DESKTOPS).context("desktops.toml")?;
    Ok(file.desktop)
}

/// Bindle's recommended desktop, pre-selected in `bindle iso`. None while the
/// would-be default is still a preview (see docs/CATALOG.md, "Recommended desktop").
pub fn recommended(desktops: &[Desktop]) -> Option<&Desktop> {
    desktops.iter().find(|d| d.status == Some(Status::Recommended) && d.broken.is_none())
}

pub fn by_id<'a>(desktops: &'a [Desktop], id: &str) -> Option<&'a Desktop> {
    desktops.iter().find(|d| d.id == id)
}

/// archinstall profile name (e.g. "KDE Plasma") → desktop.
pub fn by_archinstall_name<'a>(desktops: &'a [Desktop], name: &str) -> Option<&'a Desktop> {
    desktops.iter().filter(|d| d.base.is_none()).find(|d| d.name.eq_ignore_ascii_case(name))
}

/// The desktops a plan rounds out: `current` plus those installed by one of
/// `modules`, minus their components. None when no module installs a desktop.
pub fn with_planned(desktops: &[Desktop], current: &[String], modules: &[String]) -> Option<Vec<String>> {
    let planned: Vec<String> = desktops
        .iter()
        .filter(|d| d.module.as_ref().is_some_and(|m| modules.contains(m)))
        .map(|d| d.id.clone())
        .collect();
    if planned.is_empty() {
        return None;
    }
    let all: Vec<String> = current.iter().cloned().chain(planned).collect();
    Some(present(desktops, &all, &BTreeSet::new()))
}

/// Rules serde can't express; the catalog validator also checks `module`.
pub fn check(desktops: &[Desktop]) -> Result<()> {
    let recommended: Vec<&str> =
        desktops.iter().filter(|d| d.status == Some(Status::Recommended)).map(|d| d.id.as_str()).collect();
    if recommended.len() > 1 {
        bail!("more than one recommended desktop: {}", recommended.join(", "));
    }
    for d in desktops {
        if d.status == Some(Status::Recommended) && d.broken.is_some() {
            bail!("desktop `{}` is recommended but broken", d.id);
        }
        match &d.base {
            None if d.archinstall.is_empty() => bail!("desktop `{}` has no archinstall packages and no base", d.id),
            None => {}
            Some(b) => {
                let Some(base) = by_id(desktops, b).filter(|x| x.base.is_none()) else {
                    bail!("desktop `{}` has base `{b}`, which isn't an archinstall desktop", d.id);
                };
                if d.module.is_none() || !d.archinstall.is_empty() || !d.includes.contains(&base.id) {
                    bail!(
                        "desktop `{}` (on `{b}`) needs a module, no archinstall list, and includes = [\"{b}\"]",
                        d.id
                    );
                }
            }
        }
    }
    Ok(())
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
        assert_eq!(d.len(), 22);
        assert_eq!(by_archinstall_name(&d, "KDE Plasma").unwrap().id, "plasma");
        assert!(d.iter().all(|x| ["wayland", "xorg"].contains(&x.display.as_str())));
        for x in &d {
            for inc in &x.includes {
                assert!(by_id(&d, inc).is_some(), "{} includes unknown {inc}", x.id);
            }
        }
    }

    #[test]
    fn uq_rides_on_hyprland_and_is_a_preview_until_published() {
        let d = all().unwrap();
        check(&d).unwrap();
        let uq = by_id(&d, "uq").unwrap();
        assert_eq!((uq.name.as_str(), uq.kind.as_str(), uq.display.as_str()), ("uQ", "desktop", "wayland"));
        assert_eq!((uq.toolkit.as_str(), uq.detect.as_str()), ("qt", "uq"));
        assert_eq!(uq.profile(&d).id, "hyprland");
        assert_eq!(uq.module.as_deref(), Some("uq"));
        assert!(uq.greeter.is_some() && uq.archinstall.is_empty());
        // The switch: flip `status` to "recommended" once uq v0.2.0 is on the AUR.
        assert_eq!(uq.status, Some(Status::Preview));
        assert_eq!(uq.label(), "uQ (preview)");
        assert!(recommended(&d).is_none());
        // archinstall never logs a "uQ" profile.
        assert!(by_archinstall_name(&d, "uQ").is_none());
    }

    #[test]
    fn the_recommended_desktop_is_unique() {
        let mut d = all().unwrap();
        let set = |d: &mut [Desktop], id: &str, s| d.iter_mut().find(|x| x.id == id).unwrap().status = s;
        set(&mut d, "uq", Some(Status::Recommended));
        check(&d).unwrap();
        assert_eq!(recommended(&d).unwrap().label(), "uQ (recommended)");
        set(&mut d, "gnome", Some(Status::Recommended));
        assert!(check(&d).unwrap_err().to_string().contains("more than one recommended"));
        set(&mut d, "gnome", None);
        d.iter_mut().find(|x| x.id == "uq").unwrap().base = Some("nope".into());
        assert!(check(&d).unwrap_err().to_string().contains("base `nope`"));
    }

    #[test]
    fn uq_is_detected_and_hides_its_hyprland() {
        let d = all().unwrap();
        let installed: BTreeSet<String> = ["hyprland", "uq"].map(String::from).into();
        assert_eq!(present(&d, &["hyprland".into()], &installed), ["uq"]);
        // Planning the uq module rounds out uQ, not the Hyprland it is installed on.
        assert_eq!(with_planned(&d, &["hyprland".into()], &["uq".into()]).unwrap(), ["uq"]);
        assert!(with_planned(&d, &["hyprland".into()], &["fonts".into()]).is_none());
    }

    #[test]
    fn desktop_modules_exist_in_the_catalog() {
        let cat = crate::Catalog::arch().unwrap();
        for d in all().unwrap() {
            if let Some(m) = &d.module {
                assert!(cat.module(m).is_some(), "{} is installed by unknown module {m}", d.id);
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
