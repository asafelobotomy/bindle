//! Bindle's pinned GitHub-release packages and AppImages (catalog
//! `[[module.release]]`): which installed ones have a newer pin in this
//! Bindle. Pure; what is installed comes in as functions.

use std::collections::BTreeSet;

use bindle_core::Catalog;
use bindle_core::catalog::{AssetKind, ReleaseAsset};
use serde::Serialize;

/// Every pinned asset in the catalog, titled with its module's name.
pub fn pinned(catalog: &Catalog) -> Vec<ReleaseAsset> {
    let mut seen = BTreeSet::new();
    catalog
        .modules
        .iter()
        .flat_map(|m| m.releases.iter().map(move |r| ReleaseAsset { title: m.name.clone(), ..r.clone() }))
        .filter(|r| seen.insert(r.asset.clone()))
        .collect()
}

/// The version in a package file's name: `kyko-0.0.2-1-x86_64.pkg.tar.zst`
/// → `0.0.2-1`, `photocraft-0.21.0-linux-x86_64.pkg.tar.zst` → `0.21.0`
/// (the first part that starts with a digit, and the release number after
/// it when there is one). pacman's `vercmp` ignores a missing release.
pub fn pinned_version(a: &ReleaseAsset) -> Option<String> {
    let parts: Vec<&str> = a.asset.strip_suffix(".pkg.tar.zst")?.split('-').collect();
    let i = parts.iter().skip(1).position(|p| p.starts_with(|c: char| c.is_ascii_digit()))? + 1;
    let rel = parts.get(i + 1).filter(|r| i + 2 < parts.len() && r.chars().all(|c| c.is_ascii_digit()));
    Some(match rel {
        Some(r) => format!("{}-{r}", parts[i]),
        None => parts[i].to_string(),
    })
}

/// Where the installed copy of a pinned app came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Bindle installed it from its pinned release.
    Bindle,
    /// A package from the Arch repos (pacman knows it from a sync database).
    Repo,
    /// A package from elsewhere: the AUR or installed by hand.
    Other,
}

impl Source {
    pub fn describe(self) -> &'static str {
        match self {
            Source::Bindle => "Bindle's release",
            Source::Repo => "the Arch repos",
            Source::Other => "the AUR or a manual install",
        }
    }
}

/// An installed app whose pin in this Bindle is newer than what is there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Newer {
    pub asset: ReleaseAsset,
    /// The installed version (packages) or asset file (AppImages).
    pub installed: String,
    pub source: Source,
}

/// Installed apps whose pin is newer than what is there.
/// - `installed(name)`: the installed package's version.
/// - `at_least(a, b)`: version `a` is `b` or newer (pacman's `vercmp`).
/// - `source(name)`: where an installed package came from.
/// - `appimage_entry(asset)`: the text of the AppImage's desktop entry
///   (only Bindle writes one, so those are always Bindle's).
pub fn newer(
    assets: &[ReleaseAsset],
    installed: &dyn Fn(&str) -> Option<String>,
    at_least: &dyn Fn(&str, &str) -> bool,
    source: &dyn Fn(&str) -> Source,
    appimage_entry: &dyn Fn(&ReleaseAsset) -> Option<String>,
) -> Vec<Newer> {
    let mut out = vec![];
    for a in assets {
        let found = match a.kind() {
            Some(AssetKind::Package) => match (installed(&a.name), pinned_version(a)) {
                (Some(have), Some(pin)) if !at_least(&have, &pin) => Some((have, source(&a.name))),
                _ => None,
            },
            Some(AssetKind::AppImage) => appimage_entry(a).and_then(|e| {
                let have = e.lines().find_map(|l| l.strip_prefix("X-Bindle-Asset="))?.to_string();
                (have != a.asset).then_some((have, Source::Bindle))
            }),
            None => None,
        };
        if let Some((installed, source)) = found {
            out.push(Newer { asset: a.clone(), installed, source });
        }
    }
    out
}

/// Where each installed package came from: in a sync database (`foreign`
/// doesn't list it), recorded by a Bindle snapshot as a release install, or
/// neither. Snapshots from before the record listed releases among
/// `packages`; a foreign package there is one of Bindle's releases too.
pub fn classify(name: &str, foreign: &BTreeSet<String>, recorded: &BTreeSet<String>) -> Source {
    if !foreign.contains(name) {
        Source::Repo
    } else if recorded.contains(name) {
        Source::Bindle
    } else {
        Source::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, file: &str) -> ReleaseAsset {
        ReleaseAsset {
            repo: format!("o/{name}"),
            tag: "v2".into(),
            asset: file.into(),
            sha256: "0".repeat(64),
            name: name.into(),
            title: String::new(),
        }
    }

    #[test]
    fn versions_come_from_the_file_name() {
        assert_eq!(pinned_version(&asset("kyko", "kyko-0.0.2-1-x86_64.pkg.tar.zst")).as_deref(), Some("0.0.2-1"));
        let craft = asset("ai.storyteller.photocraft", "photocraft-0.21.0-linux-x86_64.pkg.tar.zst");
        assert_eq!(pinned_version(&craft).as_deref(), Some("0.21.0"));
        assert_eq!(pinned_version(&asset("x", "x-1.0-any.pkg.tar.zst")).as_deref(), Some("1.0"));
        assert_eq!(
            pinned_version(&asset("xzram-gui", "xzram-gui-0.4.3-1-x86_64.pkg.tar.zst")).as_deref(),
            Some("0.4.3-1")
        );
        assert_eq!(pinned_version(&asset("x", "x_1.0_amd64.AppImage")), None);
    }

    #[test]
    fn only_installed_and_older_assets_are_listed_with_their_source() {
        let assets = [
            asset("kyko", "kyko-0.0.3-1-x86_64.pkg.tar.zst"),
            asset("xzram", "xzram-0.4.3-1-x86_64.pkg.tar.zst"),
            asset("absent", "absent-1.0-1-x86_64.pkg.tar.zst"),
            asset("emobie", "emobie_0.9.0_amd64.AppImage"),
            asset("other", "other_1.0_amd64.AppImage"),
        ];
        let installed = |n: &str| match n {
            "kyko" => Some("0.0.2-1".to_string()),
            "xzram" => Some("0.5.0-1".to_string()),
            _ => None,
        };
        // A stand-in for vercmp, enough for these versions.
        let at_least = |a: &str, b: &str| a >= b;
        let entry = |a: &ReleaseAsset| match a.name.as_str() {
            "emobie" => Some("[Desktop Entry]\nX-Bindle-Asset=emobie_0.8.2_amd64.AppImage\n".to_string()),
            "other" => Some("[Desktop Entry]\nX-Bindle-Asset=other_1.0_amd64.AppImage\n".to_string()),
            _ => None,
        };
        let got = newer(&assets, &installed, &at_least, &|_| Source::Repo, &entry);
        let names: Vec<(&str, &str, Source)> =
            got.iter().map(|n| (n.asset.name.as_str(), n.installed.as_str(), n.source)).collect();
        assert_eq!(
            names,
            [("kyko", "0.0.2-1", Source::Repo), ("emobie", "emobie_0.8.2_amd64.AppImage", Source::Bindle)]
        );
    }

    #[test]
    fn sources_come_from_pacman_and_bindles_record() {
        let foreign: BTreeSet<String> = ["kyko".into(), "hauler".into()].into();
        let recorded: BTreeSet<String> = ["kyko".into(), "firefox".into()].into();
        assert_eq!(classify("kyko", &foreign, &recorded), Source::Bindle);
        assert_eq!(classify("hauler", &foreign, &recorded), Source::Other);
        assert_eq!(classify("firefox", &foreign, &recorded), Source::Repo, "in the repos, whatever installed it");
    }

    #[test]
    fn the_catalog_pins_are_listed_once_with_titles() {
        let c = Catalog::arch().unwrap();
        let p = pinned(&c);
        assert!(!p.is_empty());
        assert!(p.iter().all(|a| !a.title.is_empty()));
        let unique: BTreeSet<&str> = p.iter().map(|a| a.asset.as_str()).collect();
        assert_eq!(unique.len(), p.len());
        for a in p.iter().filter(|a| a.kind() == Some(AssetKind::Package)) {
            assert!(pinned_version(a).is_some(), "{}: can't read the version from {}", a.name, a.asset);
        }
    }
}
