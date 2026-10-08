//! Read-only system detection. Never needs root.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::desktops;
use crate::install::InstallRecord;
use crate::pacman_conf;

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
    pub root_fs: String,
    pub ssd: bool,
    pub laptop: bool,
    pub bluetooth: bool,
    pub virt: String,
    pub bootloader: String,
    pub session: String,
    pub zswap_enabled: Option<bool>,
    /// `zswap.enabled=0` on the kernel command line.
    pub zswap_cmdline_off: bool,
    /// Another /etc/sysctl.d file (e.g. XZram's) already tunes swap.
    pub swap_sysctl_owner: Option<String>,
    pub zram_active: bool,
    pub zram_config_present: bool,
    pub hibernate_resume: bool,
    pub multilib_enabled: bool,
    pub aur_helper: Option<String>,
    pub user: String,
    /// What the installer (archinstall) chose, recovered from its logs.
    pub install: InstallRecord,
    /// Desktop ids present (installer record ∪ live detection).
    pub desktops: Vec<String>,
    /// `toolkit:` / `display:` tags of the detected desktops.
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

impl Facts {
    pub fn detect() -> Self {
        let os = read("/etc/os-release");
        let installed = installed_packages();
        let desktop_data = desktops::all().unwrap_or_default();
        let install = InstallRecord::detect(&desktop_data);
        let found = desktops::present(&desktop_data, &install.desktops, &installed);
        let mut desktop_tags = BTreeSet::new();
        for d in found.iter().filter_map(|id| desktops::by_id(&desktop_data, id)) {
            desktop_tags.insert(format!("toolkit:{}", d.toolkit));
            desktop_tags.insert(format!("display:{}", d.display));
        }
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
            gpus: gpu_vendors(),
            root_fs: root_fs(&read("/proc/mounts")),
            ssd: has_ssd(),
            laptop: has_battery(),
            bluetooth: dir_nonempty("/sys/class/bluetooth"),
            virt: virt(),
            bootloader: bootloader(),
            session: std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "tty".into()),
            zswap_enabled: match read("/sys/module/zswap/parameters/enabled").trim() {
                "Y" => Some(true),
                "N" => Some(false),
                _ => None,
            },
            zswap_cmdline_off: read("/proc/cmdline").split_whitespace().any(|a| a == "zswap.enabled=0"),
            swap_sysctl_owner: swap_sysctl_owner(),
            zram_active: read("/proc/swaps").contains("/dev/zram"),
            zram_config_present: Path::new("/etc/systemd/zram-generator.conf").exists(),
            hibernate_resume: read("/proc/cmdline").split_whitespace().any(|a| a.starts_with("resume=")),
            multilib_enabled: pacman_conf::repo_enabled(&read("/etc/pacman.conf"), "multilib"),
            aur_helper: aur_helper(&installed),
            user: std::env::var("SUDO_USER").or_else(|_| std::env::var("USER")).unwrap_or_default(),
            desktops: found,
            desktop_tags: desktop_tags.into_iter().collect(),
            display_manager: display_manager(),
            networkmanager: installed.contains("networkmanager"),
            enabled_units: enabled_units(),
            installed_count: installed.len(),
            installed,
            groups: package_groups(),
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

    pub fn is_arch_family(&self) -> bool {
        self.distro_id == "arch" || self.distro_like.iter().any(|l| l == "arch")
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
                return Err(if negate { format!("{tag} detected") } else { format!("needs {tag}") });
            }
        }
        Ok(())
    }
}

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn dir_nonempty(path: &str) -> bool {
    fs::read_dir(path).is_ok_and(|mut d| d.next().is_some())
}

pub fn os_field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).map(|v| v.trim().trim_matches('"').to_string())
}

pub fn cpu_vendor(cpuinfo: &str) -> String {
    let id = cpuinfo.lines().find(|l| l.starts_with("vendor_id")).unwrap_or_default();
    if id.contains("GenuineIntel") {
        "intel".into()
    } else if id.contains("AuthenticAMD") {
        "amd".into()
    } else {
        "other".into()
    }
}

pub fn mem_total_mib(meminfo: &str) -> u64 {
    meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .map_or(0, |kib| kib / 1024)
}

/// Filesystem type of `/`; the last matching mount wins (overmounts).
pub fn root_fs(mounts: &str) -> String {
    mounts
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (_dev, mnt, fs) = (it.next()?, it.next()?, it.next()?);
            (mnt == "/").then(|| fs.to_string())
        })
        .next_back()
        .unwrap_or_else(|| "unknown".into())
}

pub fn pci_vendor_name(id: &str) -> &'static str {
    match id.trim() {
        "0x10de" => "nvidia",
        "0x1002" => "amd",
        "0x8086" => "intel",
        "0x1af4" | "0x15ad" | "0x1234" | "0x80ee" => "virtual",
        _ => "other",
    }
}

fn gpu_vendors() -> Vec<String> {
    let mut out = BTreeSet::new();
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("card") && !name.contains('-') {
                let v = read(&format!("/sys/class/drm/{name}/device/vendor"));
                out.insert(pci_vendor_name(&v).to_string());
            }
        }
    }
    out.into_iter().collect()
}

fn has_ssd() -> bool {
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return false;
    };
    entries.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        let real = ["nvme", "sd", "vd", "mmcblk"].iter().any(|p| name.starts_with(p));
        real && read(&format!("/sys/block/{name}/queue/rotational")).trim() == "0"
    })
}

fn has_battery() -> bool {
    fs::read_dir("/sys/class/power_supply")
        .is_ok_and(|d| d.flatten().any(|e| read(&format!("{}/type", e.path().display())).trim() == "Battery"))
}

fn virt() -> String {
    Command::new("systemd-detect-virt")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "none".into())
}

/// Name of the running loader from the Boot Loader Interface `LoaderInfo`
/// EFI variable (set by systemd-boot and Limine; world-readable).
pub fn loader_from_info(info: &str) -> Option<&'static str> {
    let info = info.to_ascii_lowercase();
    if info.contains("systemd-boot") {
        Some("systemd-boot")
    } else if info.contains("limine") {
        Some("limine")
    } else if info.contains("grub") {
        Some("grub")
    } else {
        None
    }
}

fn loader_info() -> Option<String> {
    let dir = fs::read_dir("/sys/firmware/efi/efivars").ok()?;
    let entry = dir.flatten().find(|e| e.file_name().to_string_lossy().starts_with("LoaderInfo-"))?;
    let raw = fs::read(entry.path()).ok()?;
    // 4-byte attribute header, then UTF-16LE.
    let units: Vec<u16> =
        raw.get(4..)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
    Some(String::from_utf16_lossy(&units))
}

fn swap_sysctl_owner() -> Option<String> {
    let dir = fs::read_dir("/etc/sysctl.d").ok()?;
    dir.flatten()
        .filter(|e| e.file_name().to_string_lossy() != "99-bindle-zram.conf")
        .find(|e| read(&e.path().to_string_lossy()).lines().any(|l| l.trim_start().starts_with("vm.swappiness")))
        .map(|e| e.path().display().to_string())
}

fn bootloader() -> String {
    if let Some(name) = loader_info().as_deref().and_then(loader_from_info) {
        return name.into();
    }
    let exists = |p: &str| Path::new(p).exists();
    if ["/boot/loader/loader.conf", "/efi/loader/loader.conf", "/boot/efi/loader/loader.conf"].iter().any(|p| exists(p))
    {
        "systemd-boot".into()
    } else if exists("/boot/limine.conf") || exists("/boot/limine/limine.conf") {
        "limine".into()
    } else if exists("/boot/grub/grub.cfg") {
        "grub".into()
    } else {
        "unknown".into()
    }
}

/// AUR helpers are usually installed as `-bin`/`-git` builds (paru-bin provides paru).
pub fn aur_helper(installed: &BTreeSet<String>) -> Option<String> {
    ["paru", "yay"]
        .into_iter()
        .find(|h| ["", "-bin", "-git"].iter().any(|suffix| installed.contains(&format!("{h}{suffix}"))))
        .map(String::from)
}

fn display_manager() -> Option<String> {
    let target = fs::read_link("/etc/systemd/system/display-manager.service").ok()?;
    let name = target.file_name()?.to_string_lossy().into_owned();
    Some(name.trim_end_matches(".service").to_string())
}

/// Units enabled via `[Install]` symlinks in /etc/systemd/system/*.wants.
fn enabled_units() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(dirs) = fs::read_dir("/etc/systemd/system") else { return out };
    for dir in dirs.flatten() {
        let name = dir.file_name().to_string_lossy().into_owned();
        if name.ends_with(".wants") || name.ends_with(".requires") {
            if let Ok(units) = fs::read_dir(dir.path()) {
                out.extend(units.flatten().map(|u| u.file_name().to_string_lossy().into_owned()));
            }
        } else if name == "display-manager.service" {
            out.insert(name);
        }
    }
    out
}

fn package_groups() -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let Ok(o) = Command::new("pacman").arg("-Sgg").output() else { return out };
    for line in String::from_utf8_lossy(&o.stdout).lines() {
        if let Some((group, pkg)) = line.split_once(' ') {
            out.entry(group.to_string()).or_default().insert(pkg.to_string());
        }
    }
    out
}

fn installed_packages() -> BTreeSet<String> {
    Command::new("pacman")
        .arg("-Qq")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_os_release() {
        let os = "NAME=\"Arch Linux\"\nID=arch\nID_LIKE=\"arch\"\n";
        assert_eq!(os_field(os, "ID").as_deref(), Some("arch"));
        assert_eq!(os_field(os, "ID_LIKE").as_deref(), Some("arch"));
        assert_eq!(os_field(os, "VERSION"), None);
    }

    #[test]
    fn parses_proc_files() {
        assert_eq!(cpu_vendor("vendor_id\t: AuthenticAMD\n"), "amd");
        assert_eq!(mem_total_mib("MemTotal:       16303428 kB\n"), 15921);
        let mounts = "rootfs / rootfs rw 0 0\n/dev/nvme0n1p2 / btrfs rw 0 0\n";
        assert_eq!(root_fs(mounts), "btrfs");
        assert_eq!(loader_from_info("systemd-boot 261.2-1-arch"), Some("systemd-boot"));
        assert_eq!(loader_from_info("Limine 9.3"), Some("limine"));
    }

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
    fn aur_helper_variants() {
        let set = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(aur_helper(&set(&["paru-bin"])).as_deref(), Some("paru"));
        assert_eq!(aur_helper(&set(&["yay-git"])).as_deref(), Some("yay"));
        assert_eq!(aur_helper(&set(&["pacman"])), None);
    }

    #[test]
    fn when_conditions() {
        let f = Facts { laptop: true, virt: "none".into(), ..Default::default() };
        assert!(f.matches(&["laptop".into(), "!virt".into()]).is_ok());
        assert_eq!(f.matches(&["ssd".into()]).unwrap_err(), "needs ssd");
        let vm = Facts { virt: "kvm".into(), ..Default::default() };
        assert_eq!(vm.matches(&["!virt".into()]).unwrap_err(), "virt detected");
    }
}
