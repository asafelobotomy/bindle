//! Modules whose concrete steps depend on detected hardware.

mod apps;
pub mod drives;
mod gpu;
mod initramfs;

pub use gpu::{nvidia_legacy, nvidia_open_supported};

use std::collections::BTreeSet;

use crate::catalog::FileSpec;
use crate::desktops;
use crate::detect::Facts;

pub const KNOWN: &[&str] =
    &["aur-helper", "desktop", "everyday", "gpu", "microcode", "nvidia-legacy", "spellcheck", "zram"];

/// AUR package bootstrapped by the `aur-helper` builtin.
pub const AUR_HELPER: &str = "paru-bin";

/// The AUR commit Bindle builds `AUR_HELPER` from, and that commit's PKGBUILD
/// sha256. The PKGBUILD's own sha256sums pin the release binary, so this pins
/// exactly what gets installed as root. Bump both together (docs/APPLY.md).
pub const AUR_HELPER_COMMIT: &str = "0056e0fbe08ed6cce4cae19fa416d328319a0841";
pub const AUR_HELPER_PKGBUILD_SHA256: &str = "c148e2dfb9214cc6abefed9b34ffb47ebc1b7043fc8e4cee9f9d29aaf58a2080";

/// Repo packages builtins can add outside desktops.toml, for
/// scripts/check-packages.py (which parses these const lists).
pub const PACKAGES: &[&str] = &["intel-ucode", "amd-ucode", "zram-generator"];
pub use apps::PACKAGES as APP_PACKAGES;
pub use gpu::PACKAGES as GPU_PACKAGES;

#[derive(Debug, Default, Clone)]
pub struct Expansion {
    pub packages: Vec<String>,
    /// AUR packages, beside the module's own `aur` list.
    pub aur: Vec<String>,
    pub files: Vec<FileSpec>,
    /// Files Bindle wrote earlier that should go (removed only while they
    /// still start with Bindle's marker, and not when a module writes them).
    pub remove_files: Vec<String>,
    /// systemd units to enable.
    pub enable: Vec<String>,
    /// `/etc/fstab` lines for drives mounted at startup (`drives`).
    pub mounts: Vec<crate::fstab::Entry>,
    pub aur_bootstrap: bool,
    pub rebuild_initramfs: bool,
    pub notes: Vec<String>,
}

pub struct Ctx<'a> {
    pub facts: &'a Facts,
    /// Multilib is enabled already or will be by this plan.
    pub multilib: bool,
}

pub fn expand(name: &str, ctx: &Ctx) -> Expansion {
    match name {
        "aur-helper" => aur_helper(ctx.facts),
        "desktop" => desktop(ctx.facts),
        "everyday" => apps::everyday(ctx.facts),
        "gpu" => gpu::gpu(ctx),
        "microcode" => microcode(ctx.facts),
        "nvidia-legacy" => gpu::nvidia_legacy_extras(ctx),
        "spellcheck" => apps::spellcheck(ctx.facts),
        "zram" => zram(ctx.facts),
        other => match other.strip_prefix(drives::BUILTIN_PREFIX) {
            Some(key) => drives::expand(key, ctx.facts),
            None => Expansion::default(),
        },
    }
}

fn aur_helper(f: &Facts) -> Expansion {
    // rebuild-detector: bindleupgrade finds AUR packages a library update broke.
    let packages = vec!["rebuild-detector".to_string()];
    match &f.aur_helper {
        Some(h) => Expansion { notes: vec![format!("{h} already installed.")], packages, ..Default::default() },
        None => Expansion { aur_bootstrap: true, packages, ..Default::default() },
    }
}

/// Round out each detected desktop: apps, portals, polkit agent, applets.
fn desktop(f: &Facts) -> Expansion {
    let data = desktops::all().expect("desktops.toml is embedded and validated with the catalog");
    let mut e = Expansion::default();
    for d in f.desktops.iter().filter_map(|id| desktops::by_id(&data, id)) {
        e.packages.extend(d.essentials.iter().cloned());
        if f.networkmanager {
            e.packages.extend(d.nm.iter().cloned());
        }
        if f.bluetooth {
            e.packages.extend(d.bt.iter().cloned());
        }
        e.notes.extend(d.notes.iter().map(|n| format!("{}: {n}", d.name)));
    }
    // One polkit agent: archinstall's Hyprland already brings polkit-kde-agent.
    if POLKIT_AGENTS.iter().any(|a| f.installed.contains(*a)) {
        e.packages.retain(|p| !POLKIT_AGENTS.contains(&p.as_str()));
    }
    let mut seen = BTreeSet::new();
    e.packages.retain(|p| seen.insert(p.clone()));
    e
}

const POLKIT_AGENTS: &[&str] =
    &["polkit-kde-agent", "hyprpolkitagent", "polkit-gnome", "lxqt-policykit", "mate-polkit", "xfce-polkit"];

fn microcode(f: &Facts) -> Expansion {
    let pkg = match f.cpu_vendor.as_str() {
        "intel" => "intel-ucode",
        "amd" => "amd-ucode",
        other => {
            return Expansion { notes: vec![format!("No microcode for CPU vendor `{other}`.")], ..Default::default() };
        }
    };
    Expansion { rebuild_initramfs: !f.installed.contains(pkg), packages: vec![pkg.into()], ..Default::default() }
}

/// Size expression for `zram-size`, ported from XZram's recommendation matrix.
pub fn zram_size_expr(ram_mib: u64) -> &'static str {
    match ram_mib {
        0..4096 => "min(ram, 4096)",
        4096..32768 => "min(ram / 2, 4096)",
        _ => "min(ram / 2, 8192)",
    }
}

pub const ZRAM_CONF: &str = "/etc/systemd/zram-generator.conf";
pub const ZRAM_SYSCTL: &str = "/etc/sysctl.d/99-bindle-zram.conf";
pub const ZSWAP_TMPFILES: &str = "/etc/tmpfiles.d/bindle-zswap.conf";

fn zram(f: &Facts) -> Expansion {
    let mut e = Expansion { packages: vec!["zram-generator".into()], ..Default::default() };
    if f.zram_config_present {
        e.notes.push(format!("{ZRAM_CONF} already exists; leaving it untouched."));
    } else {
        e.files.push(FileSpec {
            path: ZRAM_CONF.into(),
            mode: 0o644,
            content: format!(
                "# Managed by Bindle\n[zram0]\nzram-size = {}\ncompression-algorithm = zstd\nswap-priority = 100\nfs-type = swap\n",
                zram_size_expr(f.ram_mib)
            ),
        });
    }
    // Arch Wiki "Optimizing swap on zram".
    if let Some(owner) = &f.swap_sysctl_owner {
        e.notes.push(format!("Swap sysctls already set by {owner}; not adding Bindle's."));
    } else {
        e.files.push(FileSpec {
        path: ZRAM_SYSCTL.into(),
        mode: 0o644,
        content: "# Managed by Bindle\nvm.swappiness = 180\nvm.watermark_boost_factor = 0\nvm.watermark_scale_factor = 125\nvm.page-cluster = 0\n".into(),
        });
    }
    // zswap would compress pages before they reach zram; switch it off at boot
    // without touching the bootloader, unless the kernel cmdline already does.
    if !f.zswap_cmdline_off {
        e.files.push(FileSpec {
            path: ZSWAP_TMPFILES.into(),
            mode: 0o644,
            content: "# Managed by Bindle\nw- /sys/module/zswap/parameters/enabled - - - - N\n".into(),
        });
    }
    if f.hibernate_resume {
        e.notes.push("Hibernation is configured: keep a disk swap for resume; zram cannot be a resume device.".into());
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zram_tiers_match_xzram() {
        assert_eq!(zram_size_expr(2048), "min(ram, 4096)");
        assert_eq!(zram_size_expr(16000), "min(ram / 2, 4096)");
        assert_eq!(zram_size_expr(65536), "min(ram / 2, 8192)");
    }

    #[test]
    fn zram_respects_existing_config() {
        let f = Facts { zram_config_present: true, ram_mib: 16000, ..Default::default() };
        let e = expand("zram", &Ctx { facts: &f, multilib: false });
        assert!(e.files.iter().all(|x| x.path != ZRAM_CONF));
        assert!(e.files.iter().any(|x| x.path == ZRAM_SYSCTL));
    }

    #[test]
    fn zram_defers_to_existing_tuning() {
        let f = Facts {
            zswap_cmdline_off: true,
            swap_sysctl_owner: Some("/etc/sysctl.d/99-xzram.conf".into()),
            ..Default::default()
        };
        let e = expand("zram", &Ctx { facts: &f, multilib: false });
        assert!(e.files.iter().all(|x| x.path != ZRAM_SYSCTL && x.path != ZSWAP_TMPFILES));
    }

    #[test]
    fn desktop_essentials_follow_hardware() {
        let mut f = Facts { desktops: vec!["hyprland".into()], bluetooth: true, ..Default::default() };
        let e = expand("desktop", &Ctx { facts: &f, multilib: false });
        assert!(e.packages.contains(&"hyprlock".to_string()) && e.packages.contains(&"blueman".to_string()));
        assert!(!e.packages.contains(&"network-manager-applet".to_string()));
        f.networkmanager = true;
        let e = expand("desktop", &Ctx { facts: &f, multilib: false });
        assert!(e.packages.contains(&"network-manager-applet".to_string()));
    }

    #[test]
    fn only_one_polkit_agent() {
        let mut f = Facts { desktops: vec!["hyprland".into()], ..Default::default() };
        assert!(desktop(&f).packages.contains(&"hyprpolkitagent".to_string()));
        f.installed.insert("polkit-kde-agent".into());
        assert!(!desktop(&f).packages.contains(&"hyprpolkitagent".to_string()));
    }

    #[test]
    fn package_list_covers_microcode_and_zram() {
        for cpu in ["intel", "amd"] {
            let f = Facts { cpu_vendor: cpu.into(), ..Default::default() };
            let e = expand("microcode", &Ctx { facts: &f, multilib: false });
            assert!(e.packages.iter().all(|p| PACKAGES.contains(&p.as_str())));
        }
        let e = expand("zram", &Ctx { facts: &Facts::default(), multilib: false });
        assert!(e.packages.iter().all(|p| PACKAGES.contains(&p.as_str())));
    }
}
