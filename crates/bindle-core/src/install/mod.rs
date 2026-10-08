//! What the installer did: recovered from archinstall's log, or rebuilt from
//! the installer's pacman transactions when that log is missing.

mod archinstall_log;
mod pacman_log;
pub mod preset;

use std::collections::BTreeMap;
use std::fs;

use serde::Serialize;

use crate::desktops::Desktop;

pub use archinstall_log::parse as parse_archinstall_log;
pub use pacman_log::reconstruct as reconstruct_from_pacman_log;

pub const ARCHINSTALL_LOG: &str = "/var/log/archinstall/install.log";
pub const ARCHINSTALL_CONFIG: &str = "/var/log/archinstall/user_configuration.json";
pub const PACMAN_LOG: &str = "/var/log/pacman.log";

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// Nothing recognisable; facts come from the live system only.
    #[default]
    Unknown,
    /// The configuration archinstall logged (exact).
    ArchinstallLog,
    /// Rebuilt from the installer's `pacman -r /mnt` transactions (inferred).
    PacmanLog,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct InstallRecord {
    pub source: Source,
    /// "archinstall", "archinstall (likely)", "pacstrap" …
    pub installer: Option<String>,
    pub archinstall_version: Option<String>,
    pub installed_at: Option<String>,
    pub profile: Option<String>,
    /// Bindle desktop ids (see catalog/desktops.toml).
    pub desktops: Vec<String>,
    pub plasma_flavor: Option<String>,
    pub greeter: Option<String>,
    pub gfx_driver: Option<String>,
    pub kernels: Vec<String>,
    pub bootloader: Option<String>,
    pub uki: Option<bool>,
    pub root_fs: Option<String>,
    pub encryption: Option<String>,
    pub snapshots: Option<String>,
    pub zram: Option<bool>,
    pub zram_algorithm: Option<String>,
    pub network: Option<String>,
    pub audio: Option<String>,
    pub bluetooth: Option<bool>,
    pub power: Option<String>,
    pub printing: Option<bool>,
    pub firewall: Option<String>,
    pub fonts: Vec<String>,
    pub multilib: Option<bool>,
    pub pacman_parallel_downloads: Option<u64>,
    pub pacman_color: Option<bool>,
    pub hostname: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
    pub keyboard: Option<String>,
    pub extra_packages: Vec<String>,
    pub services: Vec<String>,
    pub custom_commands: usize,
}

impl InstallRecord {
    pub fn detect(desktops: &[Desktop]) -> Self {
        let read = |p: &str| fs::read_to_string(p).unwrap_or_default();
        let pacman = read(PACMAN_LOG);
        let from_pacman = pacman_log::reconstruct(&pacman, desktops);
        let logged = archinstall_log::parse(&read(ARCHINSTALL_LOG), desktops)
            .or_else(|| archinstall_log::parse_config_json(&read(ARCHINSTALL_CONFIG), desktops));
        let mut record = match (logged, from_pacman) {
            (Some(mut r), p) => {
                if let Some(p) = p {
                    r.installed_at = r.installed_at.or(p.installed_at);
                }
                r
            }
            (None, Some(p)) => p,
            (None, None) => Self::default(),
        };
        // `bindle iso` leaves this marker in the systems it installs.
        if std::path::Path::new("/etc/bindle/installed-by").exists() {
            record.installer = Some(match record.installer.as_deref() {
                Some(i) => format!("bindle + {i}"),
                None => "bindle".into(),
            });
        }
        record
    }

    /// Bindle modules the installer already set up, with a human reason.
    pub fn provided(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        let mut add = |id: &str, what: String| {
            out.insert(id.to_string(), what);
        };
        match self.audio.as_deref() {
            Some("pipewire") => add("audio-pipewire", "PipeWire".into()),
            Some("pulseaudio") => add("audio-pipewire", "PulseAudio chosen instead".into()),
            _ => {}
        }
        if self.bluetooth == Some(true) {
            add("bluetooth", "Bluetooth enabled".into());
        }
        match self.power.as_deref() {
            Some("power-profiles-daemon") => add("power-profiles", "power-profiles-daemon".into()),
            Some("tuned") => add("power-profiles", "tuned with tuned-ppd".into()),
            _ => {}
        }
        if self.printing == Some(true) {
            add("printing", "CUPS enabled".into());
        }
        match self.firewall.as_deref() {
            Some("ufw") => add("ufw", "ufw".into()),
            Some("firewalld") => add("firewalld", "firewalld".into()),
            _ => {}
        }
        if self.fonts.len() >= 5 {
            add("fonts", format!("{} font packages", self.fonts.len()));
        }
        if self.multilib == Some(true) {
            add("multilib", "[multilib] enabled".into());
        }
        if self.snapshots.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("snapper")) {
            add("snapper", "Snapper configured".into());
        }
        let label = match self.installer.as_deref() {
            Some(i) if i.contains("archinstall") => "archinstall",
            Some(i) => i.split_whitespace().next().unwrap_or(i),
            None => "installer",
        };
        out.into_iter().map(|(k, v)| (k, format!("{v} · via {label}"))).collect()
    }
}

/// Follow-ups worth doing after this particular install.
pub fn advisories(f: &crate::detect::Facts) -> Vec<String> {
    let (r, gpus, installed) = (&f.install, &f.gpus, &f.installed);
    let mut out = Vec::new();
    if r.gfx_driver.as_deref() == Some(GFX_DRIVERS[0].0) && !gpus.is_empty() {
        let extra: Vec<&str> = unneeded_gfx_packages(gpus).into_iter().filter(|p| installed.contains(*p)).collect();
        if !extra.is_empty() {
            out.push(format!(
                "\"All open-source\" graphics installed drivers for GPUs you don't have: {}",
                extra.join(" ")
            ));
        }
    }
    if !r.desktops.is_empty() && r.network.as_deref() == Some("iwd") && !installed.contains("networkmanager") {
        out.push("Networking is plain iwd; desktop network panels and applets expect NetworkManager.".into());
    }
    if r.zram == Some(true) && f.swap_sysctl_owner.is_none() {
        out.push(
            "archinstall enabled zram with default sizing and no swap tuning; the ZRAM module adds the sysctls.".into(),
        );
    }
    out
}

/// Packages archinstall installs for each graphics-driver choice.
pub const GFX_DRIVERS: &[(&str, &[&str])] = &[
    (
        "All open-source",
        &[
            "mesa",
            "xf86-video-amdgpu",
            "xf86-video-ati",
            "xf86-video-nouveau",
            "libva-intel-driver",
            "intel-media-driver",
            "vpl-gpu-rt",
            "libvpl",
            "vulkan-radeon",
            "vulkan-intel",
            "vulkan-nouveau",
        ],
    ),
    ("AMD / ATI (open-source)", &["mesa", "xf86-video-amdgpu", "xf86-video-ati", "vulkan-radeon"]),
    (
        "Intel (open-source)",
        &["mesa", "libva-intel-driver", "intel-media-driver", "vpl-gpu-rt", "libvpl", "vulkan-intel"],
    ),
    ("Nvidia (open kernel module for newer GPUs, Turing+)", &["nvidia-open-dkms", "dkms", "libva-nvidia-driver"]),
    ("Nvidia (open-source nouveau driver)", &["mesa", "xf86-video-nouveau", "vulkan-nouveau"]),
    ("VirtualBox (open-source)", &["mesa"]),
];

/// Drivers for vendors that aren't present, from an "All open-source" install.
pub fn unneeded_gfx_packages(gpus: &[String]) -> Vec<&'static str> {
    let keep = |p: &str| match p {
        "xf86-video-amdgpu" | "xf86-video-ati" | "vulkan-radeon" => gpus.iter().any(|g| g == "amd"),
        "xf86-video-nouveau" | "vulkan-nouveau" => gpus.iter().any(|g| g == "nvidia"),
        "libva-intel-driver" | "intel-media-driver" | "vpl-gpu-rt" | "libvpl" | "vulkan-intel" => {
            gpus.iter().any(|g| g == "intel")
        }
        _ => true,
    };
    GFX_DRIVERS[0].1.iter().copied().filter(|p| !keep(p)).collect()
}
