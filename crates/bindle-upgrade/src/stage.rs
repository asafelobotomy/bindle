//! The stages of an upgrade, in the order they run.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    /// Bindle itself: a newer signed release replaces this one, and the run
    /// continues with it (new pins, modules and recipes).
    Bindle,
    /// Arch news posted since the last upgrade, read before pacman runs.
    News,
    /// Room on `/`, `/boot` and the EFI partition, mains power, and uQ's
    /// Hyprland pin, checked before anything is upgraded.
    Preflight,
    /// A snapper snapshot of `/` first, when snapper is set up.
    Snapshot,
    /// Refresh the mirror list with reflector when it is old.
    Mirrors,
    Pacman,
    Aur,
    /// Pinned GitHub-release packages and AppImages from Bindle's catalog.
    Releases,
    Flatpak,
    /// fwupd: reported, installed only with `--firmware`.
    Firmware,
    /// AUR and other foreign packages broken by a library update
    /// (rebuild-detector), rebuilt.
    Rebuild,
    /// uQ upgraded: its per-user setup again, and its login screen kept.
    Uq,
    /// `.pacnew` / `.pacsave` files waiting to be merged.
    Pacfiles,
    Orphans,
    /// Services that failed since the upgrade started.
    Services,
    /// Bindle's own setup: new features for this machine, files changed
    /// since Bindle wrote them, drives that are gone.
    Setup,
    /// Whether to restart, and whether it is safe to (DKMS modules and boot
    /// images built for the new kernel).
    Reboot,
}

impl Stage {
    pub const ALL: [Stage; 17] = [
        Stage::Bindle,
        Stage::News,
        Stage::Preflight,
        Stage::Snapshot,
        Stage::Mirrors,
        Stage::Pacman,
        Stage::Aur,
        Stage::Releases,
        Stage::Flatpak,
        Stage::Firmware,
        Stage::Rebuild,
        Stage::Uq,
        Stage::Pacfiles,
        Stage::Orphans,
        Stage::Services,
        Stage::Setup,
        Stage::Reboot,
    ];

    /// The name `--skip` and `--only` take.
    pub fn id(self) -> &'static str {
        match self {
            Stage::Bindle => "bindle",
            Stage::News => "news",
            Stage::Preflight => "preflight",
            Stage::Snapshot => "snapshot",
            Stage::Mirrors => "mirrors",
            Stage::Pacman => "pacman",
            Stage::Aur => "aur",
            Stage::Releases => "releases",
            Stage::Flatpak => "flatpak",
            Stage::Firmware => "firmware",
            Stage::Rebuild => "rebuild",
            Stage::Uq => "uq",
            Stage::Pacfiles => "pacnew",
            Stage::Orphans => "orphans",
            Stage::Services => "services",
            Stage::Setup => "setup",
            Stage::Reboot => "reboot",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Stage::Bindle => "Bindle",
            Stage::News => "Arch news",
            Stage::Preflight => "Before updating",
            Stage::Snapshot => "Snapshot",
            Stage::Mirrors => "Mirrors",
            Stage::Pacman => "Packages",
            Stage::Aur => "AUR",
            Stage::Releases => "Bindle apps",
            Stage::Flatpak => "Flatpak",
            Stage::Firmware => "Firmware",
            Stage::Rebuild => "Rebuilds",
            Stage::Uq => "uQ",
            Stage::Pacfiles => "Config files",
            Stage::Orphans => "Orphans",
            Stage::Services => "Services",
            Stage::Setup => "Bindle setup",
            Stage::Reboot => "Reboot",
        }
    }

    /// Runs after the upgrades, from a fresh look at the system.
    pub fn after_upgrade(self) -> bool {
        matches!(
            self,
            Stage::Rebuild
                | Stage::Uq
                | Stage::Pacfiles
                | Stage::Orphans
                | Stage::Services
                | Stage::Setup
                | Stage::Reboot
        )
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.title())
    }
}

impl FromStr for Stage {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim().to_ascii_lowercase();
        let alias = match s.as_str() {
            "pacsave" | "pacfiles" | "config" => "pacnew",
            "repo" | "packages" | "system" => "pacman",
            "fwupd" => "firmware",
            "flatpaks" => "flatpak",
            "snapper" => "snapshot",
            "reflector" => "mirrors",
            "self" => "bindle",
            "space" | "power" | "checks" => "preflight",
            "dkms" | "restart" => "reboot",
            "units" | "failed" => "services",
            other => other,
        };
        Stage::ALL.into_iter().find(|st| st.id() == alias).ok_or_else(|| {
            let ids: Vec<&str> = Stage::ALL.iter().map(|s| s.id()).collect();
            format!("no stage `{s}`; stages: {}", ids.join(", "))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_aliases_work() {
        for s in Stage::ALL {
            assert_eq!(s.id().parse::<Stage>().unwrap(), s);
        }
        assert_eq!("fwupd".parse::<Stage>().unwrap(), Stage::Firmware);
        assert_eq!("PACSAVE".parse::<Stage>().unwrap(), Stage::Pacfiles);
        assert!("kernel".parse::<Stage>().unwrap_err().contains("stages: bindle, news"));
    }

    #[test]
    fn stages_run_in_declared_order() {
        let mut sorted = Stage::ALL;
        sorted.sort();
        assert_eq!(sorted, Stage::ALL);
        assert!(Stage::ALL.iter().skip_while(|s| !s.after_upgrade()).all(|s| s.after_upgrade()));
    }
}
