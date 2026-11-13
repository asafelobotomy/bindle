//! bindleupgrade's preferences: `/etc/bindle/upgrade.toml` (the system's),
//! then `~/.config/bindle/upgrade.toml` (yours), each value from the last
//! file that sets it. `bindleupgrade --prefs` prints them as a file to save;
//! `bindo upgrade-prefs` opens yours in an editor.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const SYSTEM: &str = "/etc/bindle/upgrade.toml";
pub const MIRROR_MAX_AGE_DAYS: i64 = 14;
pub const NEWS_DAYS: i64 = 30;

/// Where an app with a newer pinned release in Bindle's catalog is updated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Follow {
    /// Keep it on the source it came from: Bindle updates only what it
    /// installed from its pinned releases; a copy from the Arch repos, the
    /// AUR, Flatpak or elsewhere is left to that source.
    #[default]
    Source,
    /// Bindle's pinned release replaces whatever copy is installed when it is newer.
    Bindle,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    mirror_max_age_days: Option<i64>,
    news_days: Option<i64>,
    #[serde(default)]
    space: SpaceFile,
    battery_min_percent: Option<i64>,
    battery_max_packages: Option<i64>,
    firmware_refresh_days: Option<i64>,
    #[serde(default)]
    releases: ReleasesFile,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpaceFile {
    root_mb: Option<i64>,
    boot_mb: Option<i64>,
    efi_mb: Option<i64>,
}

/// Free space the package stages need, in MB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Space {
    /// On `/`: the downloads and the unpacked packages.
    pub root_mb: i64,
    /// On a separate `/boot`: new kernel and initramfs images (linux-zen
    /// with NVIDIA's DKMS modules makes large ones).
    pub boot_mb: i64,
    /// On the EFI partition, when the images or the boot loader live there.
    pub efi_mb: i64,
}

impl Default for Space {
    fn default() -> Self {
        Space { root_mb: 3072, boot_mb: 200, efi_mb: 100 }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleasesFile {
    follow: Option<Follow>,
    #[serde(default)]
    apps: BTreeMap<String, Follow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Prefs {
    /// Refresh the mirror list (reflector) when it is this many days old.
    pub mirror_max_age_days: i64,
    /// Without a past upgrade in pacman.log, show this many days of news.
    pub news_days: i64,
    pub follow: Follow,
    /// Per app (package or AppImage name), over `follow`.
    pub apps: BTreeMap<String, Follow>,
    pub space: Space,
    /// On battery below this charge, a package upgrade waits for mains power.
    pub battery_min_percent: i64,
    /// On battery, an upgrade this big (or with a kernel or driver) waits too.
    pub battery_max_packages: i64,
    /// Refresh fwupd's metadata before checking when it is this many days old.
    pub firmware_refresh_days: i64,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            mirror_max_age_days: MIRROR_MAX_AGE_DAYS,
            news_days: NEWS_DAYS,
            follow: Follow::Source,
            apps: BTreeMap::new(),
            space: Space::default(),
            battery_min_percent: 40,
            battery_max_packages: 30,
            firmware_refresh_days: 7,
        }
    }
}

impl Prefs {
    pub fn follow(&self, app: &str) -> Follow {
        self.apps.get(app).copied().unwrap_or(self.follow)
    }

    /// Apply one file's text over these; an error names what is wrong.
    pub fn merge(&mut self, text: &str) -> Result<(), String> {
        let f: File = toml::from_str(text).map_err(|e| e.to_string())?;
        let ranges = [
            ("mirror_max_age_days", f.mirror_max_age_days, 3650),
            ("news_days", f.news_days, 3650),
            ("firmware_refresh_days", f.firmware_refresh_days, 3650),
            ("battery_min_percent", f.battery_min_percent, 100),
            ("battery_max_packages", f.battery_max_packages, 100_000),
            ("space.root_mb", f.space.root_mb, 1_000_000),
            ("space.boot_mb", f.space.boot_mb, 1_000_000),
            ("space.efi_mb", f.space.efi_mb, 1_000_000),
        ];
        for (name, v, max) in ranges {
            if v.is_some_and(|d| !(0..=max).contains(&d)) {
                return Err(format!("{name} is from 0 to {max}"));
            }
        }
        self.space.root_mb = f.space.root_mb.unwrap_or(self.space.root_mb);
        self.space.boot_mb = f.space.boot_mb.unwrap_or(self.space.boot_mb);
        self.space.efi_mb = f.space.efi_mb.unwrap_or(self.space.efi_mb);
        self.battery_min_percent = f.battery_min_percent.unwrap_or(self.battery_min_percent);
        self.battery_max_packages = f.battery_max_packages.unwrap_or(self.battery_max_packages);
        self.firmware_refresh_days = f.firmware_refresh_days.unwrap_or(self.firmware_refresh_days);
        self.mirror_max_age_days = f.mirror_max_age_days.unwrap_or(self.mirror_max_age_days);
        self.news_days = f.news_days.unwrap_or(self.news_days);
        self.follow = f.releases.follow.unwrap_or(self.follow);
        self.apps.extend(f.releases.apps);
        Ok(())
    }

    /// As a file to save, with what each value means.
    pub fn to_toml(&self) -> String {
        let word = |f: Follow| if f == Follow::Bindle { "bindle" } else { "source" };
        let mut s = format!(
            "# bindleupgrade preferences (docs/UPGRADE.md)\n\n\
             # Refresh the mirror list with reflector when it is this many days old.\n\
             mirror_max_age_days = {}\n\
             # Without a past upgrade in pacman.log, show this many days of Arch news.\n\
             news_days = {}\n\
             # Refresh fwupd's firmware list before checking when it is this many days old.\n\
             firmware_refresh_days = {}\n\
             # On battery, package upgrades wait for mains power below this charge, or\n\
             # when they are this big or bring a kernel or driver.\n\
             battery_min_percent = {}\n\
             battery_max_packages = {}\n\n\
             [space]\n\
             # Free space (MB) the package upgrades need; less holds them back.\n\
             root_mb = {}\n\
             boot_mb = {}\n\
             efi_mb = {}\n\n\
             [releases]\n\
             # Apps Bindle pins to a GitHub release, when the pin is newer:\n\
             #   \"source\": update only copies Bindle installed; leave ones from the Arch\n\
             #             repos, the AUR or elsewhere to that source.\n\
             #   \"bindle\": Bindle's release replaces any installed copy.\n\
             follow = \"{}\"\n\n\
             [releases.apps]\n\
             # Per app, over `follow`, e.g. kyko = \"bindle\"\n",
            self.mirror_max_age_days,
            self.news_days,
            self.firmware_refresh_days,
            self.battery_min_percent,
            self.battery_max_packages,
            self.space.root_mb,
            self.space.boot_mb,
            self.space.efi_mb,
            word(self.follow)
        );
        for (app, f) in &self.apps {
            s.push_str(&format!("{app} = \"{}\"\n", word(*f)));
        }
        s
    }
}

/// Your file: `$XDG_CONFIG_HOME/bindle/upgrade.toml`.
pub fn user_path() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|c| c.join("bindle/upgrade.toml"))
}

/// The system's file, then yours. A broken file is skipped with a warning.
pub fn load(warn: &mut dyn FnMut(&str)) -> Prefs {
    let mut p = Prefs::default();
    for path in [Some(PathBuf::from(SYSTEM)), user_path()].into_iter().flatten() {
        if let Ok(text) = fs::read_to_string(&path)
            && let Err(e) = p.merge(&text)
        {
            warn(&format!("ignored {}: {e}", path.display()));
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_files_win_value_by_value() {
        let mut p = Prefs::default();
        p.merge("news_days = 7\n[releases]\nfollow = \"bindle\"\n[releases.apps]\nkyko = \"source\"\n").unwrap();
        p.merge("mirror_max_age_days = 3\n[releases.apps]\nemobie = \"source\"\n").unwrap();
        assert_eq!((p.news_days, p.mirror_max_age_days), (7, 3));
        assert_eq!(p.follow("kyko"), Follow::Source);
        assert_eq!(p.follow("emobie"), Follow::Source);
        assert_eq!(p.follow("xzram"), Follow::Bindle);
    }

    #[test]
    fn bad_files_are_refused() {
        let mut p = Prefs::default();
        assert!(p.merge("follow = \"bindle\"").is_err(), "belongs under [releases]");
        assert!(p.merge("news_days = -1").is_err());
        assert!(p.merge("battery_min_percent = 101").is_err());
        assert!(p.merge("[space]\nboot = 3").is_err());
        assert!(p.merge("[releases]\nfollow = \"aur\"").is_err());
        assert_eq!(p, Prefs::default());
    }

    #[test]
    fn the_printed_file_reads_back_the_same() {
        let mut p = Prefs { news_days: 10, follow: Follow::Bindle, battery_min_percent: 20, ..Default::default() };
        p.space.boot_mb = 500;
        p.apps.insert("kyko".into(), Follow::Source);
        let mut back = Prefs::default();
        back.merge(&p.to_toml()).unwrap();
        assert_eq!(back, p);
    }
}
