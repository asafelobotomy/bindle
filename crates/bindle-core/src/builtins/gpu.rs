//! Graphics drivers for the detected GPUs.

use std::collections::BTreeSet;

use super::{Ctx, Expansion};
use crate::catalog::FileSpec;
use crate::detect::{Facts, TURING_FIRST_DEVICE};

/// Every repo package this builtin can add (checked by scripts/check-packages.py).
pub const PACKAGES: &[&str] = &[
    "mesa",
    "lib32-mesa",
    "vulkan-radeon",
    "lib32-vulkan-radeon",
    "vulkan-intel",
    "lib32-vulkan-intel",
    "intel-media-driver",
    "vulkan-nouveau",
    "lib32-vulkan-nouveau",
    "nvidia-open-dkms",
    "nvidia-utils",
    "nvidia-settings",
    "libva-nvidia-driver",
    "lib32-nvidia-utils",
    "nvidia-prime",
    "switcheroo-control",
    "linux-headers",
    "linux-lts-headers",
    "linux-zen-headers",
    "linux-hardened-headers",
];

const KERNELS: &[&str] = &["linux", "linux-lts", "linux-zen", "linux-hardened"];

/// nvidia-open supports Turing and newer. With no device ids (unknown),
/// assume a current card.
pub fn nvidia_open_supported(f: &Facts) -> bool {
    f.nvidia_devices.is_empty() || f.nvidia_devices.iter().any(|&d| d >= TURING_FIRST_DEVICE)
}

/// First PCI device id of the Maxwell generation (GM107). From here up to
/// Turing (Maxwell, Pascal, Volta) NVIDIA's own driver is the 580 branch,
/// on the AUR as nvidia-580xx-dkms. Older cards (Kepler, Fermi) share id
/// ranges and need other branches, so they aren't matched here.
const MAXWELL_FIRST_DEVICE: u16 = 0x1340;

/// This machine's NVIDIA GPU needs the legacy 580 driver branch.
pub fn nvidia_legacy(f: &Facts) -> bool {
    f.gpus.iter().any(|g| g == "nvidia")
        && !f.nvidia_devices.is_empty()
        && !nvidia_open_supported(f)
        && f.nvidia_devices.iter().any(|&d| d >= MAXWELL_FIRST_DEVICE)
}

/// What the `nvidia-legacy` module needs beside its AUR packages: headers
/// for DKMS to build against, and the 32-bit libraries with multilib.
pub(super) fn nvidia_legacy_extras(ctx: &Ctx) -> Expansion {
    let mut e = Expansion { packages: kernel_headers(ctx.facts), ..Default::default() };
    if ctx.multilib {
        e.aur.push("lib32-nvidia-580xx-utils".into());
    }
    super::initramfs::nvidia(ctx.facts, true, &mut e);
    // The 580 branch's closed modules keep video memory over suspend only
    // through these services (595+'s open modules don't need them).
    e.enable.extend(["nvidia-suspend.service", "nvidia-resume.service", "nvidia-hibernate.service"].map(String::from));
    if !ctx.facts.installed.contains("nvidia-580xx-dkms") {
        e.notes.push(
            "NVIDIA's 580 driver comes from the AUR; DKMS builds it for your kernel (a few minutes) and it \
             replaces nouveau after a restart. Untick this module to stay on the open-source nouveau driver."
                .into(),
        );
    }
    e
}

pub(super) fn gpu(ctx: &Ctx) -> Expansion {
    let f = ctx.facts;
    let mut e = Expansion::default();
    let add = |e: &mut Expansion, pkgs: &[&str], lib32: &[&str]| {
        e.packages.extend(pkgs.iter().map(|p| p.to_string()));
        if ctx.multilib {
            e.packages.extend(lib32.iter().map(|p| format!("lib32-{p}")));
        }
    };
    let existing = installed_nvidia_driver(f);
    // On derivatives without a driver yet, don't add PRIME tools for one.
    let proprietary = existing.is_some() || (f.is_arch() && nvidia_open_supported(f));
    for gpu in &f.gpus {
        match gpu.as_str() {
            "amd" => add(&mut e, &["mesa", "vulkan-radeon"], &["mesa", "vulkan-radeon"]),
            "intel" => {
                add(&mut e, &["mesa", "vulkan-intel", "intel-media-driver"], &["mesa", "vulkan-intel"]);
                // intel-media-driver starts at Broadwell; older GPUs decode
                // video with i965. Quick Sync (oneVPL) runs on the newer ones.
                let ids = &f.extras.intel_devices;
                if ids.iter().any(|d| crate::detect::extras::intel_gen_old(*d)) {
                    e.packages.push("libva-intel-driver".into());
                }
                if ids.iter().any(|d| !crate::detect::extras::intel_gen_old(*d)) {
                    e.packages.push("vpl-gpu-rt".into());
                }
            }
            // Adding nvidia-open-dkms (or nouveau's Vulkan driver) next to an
            // installed NVIDIA kernel module conflicts, and under --noconfirm
            // that aborts the whole package transaction.
            "nvidia" if existing.is_some() => {
                let driver = existing.unwrap_or_default();
                e.notes.push(if driver.contains("open") && !nvidia_open_supported(f) {
                    format!(
                        "The installed NVIDIA driver ({driver}) can't drive this pre-Turing GPU. \
                         Replace it with nvidia-580xx-dkms and nvidia-580xx-utils from the AUR \
                         (Kepler: nvidia-470xx-dkms)."
                    )
                } else {
                    format!("Keeping the installed NVIDIA driver ({driver}).")
                });
            }
            // Derivatives package NVIDIA drivers per kernel and ship their own
            // driver managers; nvidia-open-dkms may be missing or conflict.
            "nvidia" if !f.is_arch() => e.notes.push(
                "NVIDIA GPU on an Arch derivative: install the driver with your distro's tool \
                 (Manjaro: mhwd, CachyOS: chwd, EndeavourOS: nvidia-inst)."
                    .into(),
            ),
            "nvidia" if proprietary => {
                // libva-nvidia-driver: VA-API (browser/video) decoding via NVDEC.
                add(
                    &mut e,
                    &["nvidia-open-dkms", "nvidia-utils", "nvidia-settings", "libva-nvidia-driver"],
                    &["nvidia-utils"],
                );
                e.packages.extend(kernel_headers(f));
                e.notes.push("nvidia-open supports Turing (GTX 16xx / RTX 20xx) and newer GPUs.".into());
                laptop_power(f, &mut e);
            }
            "nvidia" => {
                add(&mut e, &["mesa", "vulkan-nouveau"], &["mesa", "vulkan-nouveau"]);
                e.notes.push(
                    if nvidia_legacy(f) {
                        "This NVIDIA GPU predates Turing, so nvidia-open cannot drive it. NVIDIA's own driver for it \
                         is the \"NVIDIA driver for older cards\" module (recommended); nouveau is installed as the open-source fallback."
                    } else {
                        "This NVIDIA GPU predates Turing, so nvidia-open cannot drive it: using the open nouveau driver. \
                         For NVIDIA's driver, Kepler cards need nvidia-470xx-dkms from the AUR."
                    }
                    .into(),
                );
            }
            "virtual" => {
                e.packages.push("mesa".into());
                e.notes.push("Virtual GPU detected: installing Mesa only.".into());
            }
            _ => {}
        }
    }
    if f.hybrid_gpu() {
        if proprietary {
            e.packages.push("nvidia-prime".into());
        }
        e.packages.push("switcheroo-control".into());
        e.enable.push("switcheroo-control.service".into());
        e.notes.push(if proprietary {
            "Hybrid graphics: the integrated GPU drives the desktop; start apps on the NVIDIA GPU with `prime-run <app>` or your desktop's \"Launch using Discrete Graphics Card\".".into()
        } else {
            "Hybrid graphics: the integrated GPU drives the desktop; start apps on the NVIDIA GPU with `DRI_PRIME=1 <app>` or your desktop's \"Launch using Discrete Graphics Card\".".into()
        });
    }
    if f.gpus.is_empty() {
        e.notes.push("No GPU detected; skipping graphics drivers.".into());
    }
    // NVIDIA's own driver keeps nouveau out of the initramfs; nouveau undoes that.
    let nvidia_driver = f.gpus.iter().any(|g| g == "nvidia") && proprietary;
    super::initramfs::nvidia(f, nvidia_driver, &mut e);
    let mut seen = BTreeSet::new();
    e.packages.retain(|p| seen.insert(p.clone()));
    e
}

/// An installed NVIDIA driver: a kernel-module package (`nvidia`,
/// `nvidia-open`, `nvidia-dkms`, `nvidia-open-lts`, `nvidia-580xx-dkms`, or
/// kernel-prefixed ones on derivatives such as `linux612-nvidia-580xx`), or a
/// legacy branch's userspace (`nvidia-580xx-utils`, which conflicts with
/// `nvidia-utils`). Plain `nvidia-utils`/`nvidia-settings` don't count.
fn installed_nvidia_driver(f: &Facts) -> Option<&str> {
    let legacy = |t: &str| t.strip_suffix("xx").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    let module_suffix = |rest: &str| {
        rest.is_empty()
            || rest.strip_prefix('-').is_some_and(|rest| {
                rest.split('-')
                    .all(|t| matches!(t, "open" | "dkms" | "lts" | "beta" | "zen" | "hardened" | "git") || legacy(t))
            })
    };
    let is_driver = |p: &str| {
        if let Some(rest) = p.strip_prefix("nvidia") {
            return module_suffix(rest)
                || rest.strip_prefix('-').and_then(|r| r.strip_suffix("-utils")).is_some_and(legacy);
        }
        p.split_once("-nvidia").is_some_and(|(kernel, rest)| {
            kernel.starts_with("linux")
                && !kernel.contains("firmware")
                && !kernel.contains("headers")
                && module_suffix(rest)
        })
    };
    f.installed.iter().map(String::as_str).find(|p| is_driver(p))
}

/// The NVIDIA GPU of a hybrid laptop powered off when idle: Turing needs
/// fine-grained power management asked for (Ampere and newer have it by
/// default); Ampere and newer also get Dynamic Boost (nvidia-powerd).
fn laptop_power(f: &Facts, e: &mut Expansion) {
    if !f.laptop || !f.hybrid_gpu() {
        return;
    }
    if f.nvidia_devices.iter().any(|d| (TURING_FIRST_DEVICE..AMPERE_FIRST_DEVICE).contains(d)) {
        e.files.push(FileSpec {
            path: NVIDIA_PM.into(),
            content: "# Managed by Bindle: power the NVIDIA GPU off when idle (Turing)\noptions nvidia \"NVreg_DynamicPowerManagement=0x02\"\n".into(),
            mode: 0o644,
        });
        e.rebuild_initramfs = true;
    }
    if f.nvidia_devices.iter().any(|d| *d >= AMPERE_FIRST_DEVICE) {
        e.enable.push("nvidia-powerd.service".into());
    }
}

/// Ampere (RTX 30xx) and newer start here.
pub const AMPERE_FIRST_DEVICE: u16 = 0x2200;
pub const NVIDIA_PM: &str = "/etc/modprobe.d/bindle-nvidia-pm.conf";

pub(super) fn kernel_headers(f: &Facts) -> Vec<String> {
    let found: Vec<String> =
        KERNELS.iter().filter(|k| f.installed.contains(**k)).map(|k| format!("{k}-headers")).collect();
    if found.is_empty() { vec!["linux-headers".into()] } else { found }
}

#[cfg(test)]
#[path = "gpu_tests.rs"]
mod tests;
