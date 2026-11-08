//! Read-only system detection. Never needs root.
//!
//! `hw` covers CPU/GPU/storage/power, `boot` the bootloader and kernel
//! command line, `system` packages, services, users and config files.

mod boot;
mod hw;
mod system;
mod tags;

pub use tags::label as tag_label;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use serde::Serialize;

use crate::desktops;
use crate::install::InstallRecord;
use crate::pacman_conf;

pub use boot::{bootloader_from, loader_from_info};
pub use hw::{GpuInfo, TURING_FIRST_DEVICE, classify_gpus, cpu_vendor, is_portable, mem_total_mib, pci_vendor_name};
pub use system::{aur_helper, os_field, root_fs, snapshots_mounted, sysctl_owner, zram_config_paths};

#[derive(Debug, Clone, Default, Serialize)]
pub struct Facts {
    pub distro_id: String,
    pub distro_like: Vec<String>,
    pub pretty_name: String,
    pub kernel: String,
    pub cpu_vendor: String,
    pub cpu_cores: usize,
    pub ram_mib: u64,
    pub gpus: Vec<String>,
    /// PCI device ids of NVIDIA GPUs (decides nvidia-open vs. nouveau).
    pub nvidia_devices: Vec<u16>,
    pub root_fs: String,
    /// `/.snapshots` is its own mount (archinstall's `@.snapshots` subvolume).
    pub snapshots_mount: bool,
    pub ssd: bool,
    pub laptop: bool,
    pub bluetooth: bool,
    pub virt: String,
    pub bootloader: String,
    pub session: String,
    pub zswap_enabled: Option<bool>,
    /// `zswap.enabled=0` on the kernel command line.
    pub zswap_cmdline_off: bool,
    /// Another sysctl.d file (e.g. XZram's) already tunes swap.
    pub swap_sysctl_owner: Option<String>,
    pub zram_active: bool,
    pub zram_config_present: bool,
    pub hibernate_resume: bool,
    pub multilib_enabled: bool,
    pub aur_helper: Option<String>,
    pub user: String,
    /// Home directory of `user` (from the passwd database).
    pub home: String,
    /// Groups `user` already belongs to.
    pub user_groups: BTreeSet<String>,
    /// What the installer (archinstall) chose, recovered from its logs.
    pub install: InstallRecord,
    /// Desktop ids present (installer record ∪ live detection).
    pub desktops: Vec<String>,
    /// Tags derived from the detected desktops: `toolkit:`, `display:`, and
    /// `kdeconnect` when a desktop's essentials include KDE Connect.
    pub desktop_tags: Vec<String>,
    /// Unit behind display-manager.service, e.g. `gdm`.
    pub display_manager: Option<String>,
    pub networkmanager: bool,
    #[serde(skip)]
    pub enabled_units: BTreeSet<String>,
    #[serde(skip)]
    pub installed: BTreeSet<String>,
    /// Sync-database package groups → members (`pacman -Sgg`).
    #[serde(skip)]
    pub groups: BTreeMap<String, BTreeSet<String>>,
    pub installed_count: usize,
}

/// Boolean tags `Facts::tags` can emit (validated against the catalog).
pub const FLAG_TAGS: &[&str] = &[
    "ssd",
    "laptop",
    "bluetooth",
    "virt",
    "multilib",
    "zram-active",
    "zswap",
    "hibernate",
    "aur-helper",
    "desktop",
    "nm",
    "kdeconnect",
    "snapshots-mount",
    "hybrid-gpu",
];

/// `prefix:value` tags; `None` means any value is accepted.
pub const VALUE_TAGS: &[(&str, Option<&[&str]>)] = &[
    ("distro", None),
    ("cpu", Some(&["intel", "amd", "other"])),
    ("gpu", Some(&["nvidia", "amd", "intel", "virtual", "other"])),
    ("fs", None),
    ("boot", Some(&["systemd-boot", "grub", "limine", "refind", "unknown"])),
    ("session", Some(&["wayland", "x11", "tty"])),
    ("desktop", None),
    ("toolkit", Some(&["qt", "gtk", "other"])),
    ("display", Some(&["wayland", "xorg"])),
    ("installer", Some(&["archinstall", "pacstrap"])),
];

impl Facts {
    pub fn detect() -> Self {
        let os = read("/etc/os-release");
        let cmdline = read("/proc/cmdline");
        let installed = system::installed_packages();
        let desktop_data = desktops::all().unwrap_or_default();
        let install = InstallRecord::detect(&desktop_data);
        let found = desktops::present(&desktop_data, &install.desktops, &installed);
        let mut desktop_tags = BTreeSet::new();
        for d in found.iter().filter_map(|id| desktops::by_id(&desktop_data, id)) {
            desktop_tags.insert(format!("toolkit:{}", d.toolkit));
            desktop_tags.insert(format!("display:{}", d.display));
            if d.essentials.iter().any(|p| p == "kdeconnect") {
                desktop_tags.insert("kdeconnect".into());
            }
        }
        let virt = hw::virt();
        let gpu = hw::gpus(&virt);
        let user = system::current_user();
        let mounts = read("/proc/mounts");
        let mut f = Facts {
            distro_id: os_field(&os, "ID").unwrap_or_default(),
            distro_like: os_field(&os, "ID_LIKE")
                .map(|s| s.split_whitespace().map(String::from).collect())
                .unwrap_or_default(),
            pretty_name: os_field(&os, "PRETTY_NAME").unwrap_or_default(),
            kernel: read("/proc/sys/kernel/osrelease").trim().to_string(),
            cpu_vendor: cpu_vendor(&read("/proc/cpuinfo")),
            cpu_cores: std::thread::available_parallelism().map_or(1, |n| n.get()),
            ram_mib: mem_total_mib(&read("/proc/meminfo")),
            gpus: gpu.vendors,
            nvidia_devices: gpu.nvidia_devices,
            root_fs: root_fs(&mounts),
            snapshots_mount: snapshots_mounted(&mounts),
            ssd: hw::has_ssd(),
            laptop: hw::has_battery(),
            bluetooth: dir_nonempty("/sys/class/bluetooth"),
            virt,
            bootloader: boot::bootloader(&installed),
            session: std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "tty".into()),
            zswap_enabled: match read("/sys/module/zswap/parameters/enabled").trim() {
                "Y" => Some(true),
                "N" => Some(false),
                _ => None,
            },
            zswap_cmdline_off: cmdline.split_whitespace().any(|a| a == "zswap.enabled=0"),
            swap_sysctl_owner: system::swap_sysctl_owner(),
            zram_active: read("/proc/swaps").contains("/dev/zram"),
            zram_config_present: !zram_config_paths(&|d| system::list_dir(d)).is_empty(),
            hibernate_resume: cmdline.split_whitespace().any(|a| a.starts_with("resume=")),
            multilib_enabled: pacman_conf::repo_enabled(&read("/etc/pacman.conf"), "multilib"),
            aur_helper: aur_helper(&installed),
            user_groups: system::user_groups(&user),
            home: system::user_home(&user),
            user,
            desktops: found,
            desktop_tags: desktop_tags.into_iter().collect(),
            display_manager: system::display_manager(),
            networkmanager: installed.contains("networkmanager"),
            enabled_units: system::enabled_units(),
            installed_count: installed.len(),
            installed,
            groups: system::package_groups(),
            install,
        };
        if f.distro_id.is_empty() {
            f.distro_id = "unknown".into();
        }
        if f.bootloader == "unknown"
            && let Some(b) = &f.install.bootloader
        {
            f.bootloader = b.to_lowercase();
        }
        f
    }

    /// Installed, or a group whose members are all installed.
    pub fn has_package(&self, name: &str) -> bool {
        self.installed.contains(name)
            || self.groups.get(name).is_some_and(|m| !m.is_empty() && m.iter().all(|p| self.installed.contains(p)))
    }

    /// Arch Linux itself (or unknown, as in tests): not a derivative.
    pub fn is_arch(&self) -> bool {
        matches!(self.distro_id.as_str(), "arch" | "" | "unknown")
    }

    pub fn is_arch_family(&self) -> bool {
        self.distro_id == "arch" || self.distro_like.iter().any(|l| l == "arch")
    }

    /// An NVIDIA GPU next to an integrated/other Intel or AMD one.
    pub fn hybrid_gpu(&self) -> bool {
        self.gpus.iter().any(|g| g == "nvidia") && self.gpus.iter().any(|g| g == "intel" || g == "amd")
    }

    /// Tags matched by a module's `when` list.
    pub fn tags(&self) -> BTreeSet<String> {
        let mut t = BTreeSet::new();
        t.insert(format!("distro:{}", self.distro_id));
        t.insert(format!("cpu:{}", self.cpu_vendor));
        t.insert(format!("fs:{}", self.root_fs));
        t.insert(format!("boot:{}", self.bootloader));
        t.insert(format!("session:{}", self.session));
        for g in &self.gpus {
            t.insert(format!("gpu:{g}"));
        }
        for d in &self.desktops {
            t.insert(format!("desktop:{d}"));
        }
        t.extend(self.desktop_tags.iter().cloned());
        if let Some(i) = &self.install.installer {
            t.insert(format!("installer:{}", i.split_whitespace().next().unwrap_or(i)));
        }
        for (flag, name) in [
            (self.ssd, "ssd"),
            (self.laptop, "laptop"),
            (self.bluetooth, "bluetooth"),
            (self.virt != "none", "virt"),
            (self.multilib_enabled, "multilib"),
            (self.zram_active, "zram-active"),
            (self.zswap_enabled == Some(true), "zswap"),
            (self.hibernate_resume, "hibernate"),
            (self.aur_helper.is_some(), "aur-helper"),
            (!self.desktops.is_empty(), "desktop"),
            (self.networkmanager, "nm"),
            (self.installed.contains("kdeconnect"), "kdeconnect"),
            (self.snapshots_mount, "snapshots-mount"),
            (self.hybrid_gpu(), "hybrid-gpu"),
        ] {
            if flag {
                t.insert(name.into());
            }
        }
        t
    }

    pub fn matches(&self, conditions: &[String]) -> Result<(), String> {
        let tags = self.tags();
        for c in conditions {
            let (negate, tag) = c.strip_prefix('!').map_or((false, c.as_str()), |t| (true, t));
            if tags.contains(tag) == negate {
                return Err(tags::reason(tag, negate));
            }
        }
        Ok(())
    }
}

/// Whether `tag` (without `!`) is one `Facts::tags` can produce.
pub fn known_tag(tag: &str) -> bool {
    match tag.split_once(':') {
        None => FLAG_TAGS.contains(&tag),
        Some((prefix, value)) => VALUE_TAGS
            .iter()
            .find(|(p, _)| *p == prefix)
            .is_some_and(|(_, values)| !value.is_empty() && values.is_none_or(|v| v.contains(&value))),
    }
}

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn dir_nonempty(path: &str) -> bool {
    fs::read_dir(path).is_ok_and(|mut d| d.next().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_count_as_installed_when_complete() {
        let mut f = Facts::default();
        f.groups.insert("deepin-extra".into(), ["a", "b"].map(String::from).into());
        f.installed.insert("a".into());
        assert!(!f.has_package("deepin-extra"));
        f.installed.insert("b".into());
        assert!(f.has_package("deepin-extra") && f.has_package("a"));
    }

    #[test]
    fn when_conditions() {
        let f = Facts { laptop: true, virt: "none".into(), ..Default::default() };
        assert!(f.matches(&["laptop".into(), "!virt".into()]).is_ok());
        assert_eq!(f.matches(&["ssd".into()]).unwrap_err(), "needs an SSD");
        let vm = Facts { virt: "kvm".into(), ..Default::default() };
        assert_eq!(vm.matches(&["!virt".into()]).unwrap_err(), "not for a virtual machine");
    }

    #[test]
    fn every_emitted_tag_is_known() {
        let mut f = Facts {
            distro_id: "arch".into(),
            cpu_vendor: "amd".into(),
            root_fs: "btrfs".into(),
            bootloader: "limine".into(),
            session: "wayland".into(),
            gpus: vec!["nvidia".into(), "intel".into()],
            desktops: vec!["plasma".into()],
            desktop_tags: vec!["toolkit:qt".into(), "display:wayland".into(), "kdeconnect".into()],
            ssd: true,
            laptop: true,
            bluetooth: true,
            virt: "kvm".into(),
            multilib_enabled: true,
            zram_active: true,
            zswap_enabled: Some(true),
            hibernate_resume: true,
            aur_helper: Some("paru".into()),
            networkmanager: true,
            snapshots_mount: true,
            ..Default::default()
        };
        f.install.installer = Some("archinstall (likely)".into());
        let tags = f.tags();
        assert!(tags.contains("hybrid-gpu"));
        for t in &tags {
            assert!(known_tag(t), "unknown tag {t}");
        }
        assert!(!known_tag("gpu:matrox") && !known_tag("laptops") && known_tag("fs:ext4"));
    }
}
