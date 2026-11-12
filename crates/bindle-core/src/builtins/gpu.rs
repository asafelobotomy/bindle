//! Graphics drivers for the detected GPUs.

use std::collections::BTreeSet;

use super::{Ctx, Expansion};
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
            "intel" => add(&mut e, &["mesa", "vulkan-intel", "intel-media-driver"], &["mesa", "vulkan-intel"]),
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
            }
            "nvidia" => {
                add(&mut e, &["mesa", "vulkan-nouveau"], &["mesa", "vulkan-nouveau"]);
                e.notes.push(
                    "This NVIDIA GPU predates Turing, so nvidia-open cannot drive it: using the open nouveau driver. \
                     For NVIDIA's driver, Maxwell–Volta cards need nvidia-580xx-dkms from the AUR (Kepler: nvidia-470xx-dkms)."
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

fn kernel_headers(f: &Facts) -> Vec<String> {
    let found: Vec<String> =
        KERNELS.iter().filter(|k| f.installed.contains(**k)).map(|k| format!("{k}-headers")).collect();
    if found.is_empty() { vec!["linux-headers".into()] } else { found }
}

#[cfg(test)]
mod tests {
    use super::super::expand;
    use super::*;

    fn run(f: &Facts, multilib: bool) -> Expansion {
        expand("gpu", &Ctx { facts: f, multilib })
    }

    fn nvidia(devices: &[u16]) -> Facts {
        Facts { gpus: vec!["nvidia".into()], nvidia_devices: devices.to_vec(), ..Default::default() }
    }

    #[test]
    fn adds_lib32_only_with_multilib() {
        let f = Facts { gpus: vec!["amd".into()], ..Default::default() };
        assert!(!run(&f, false).packages.iter().any(|p| p.starts_with("lib32-")));
        assert!(run(&f, true).packages.contains(&"lib32-vulkan-radeon".to_string()));
    }

    #[test]
    fn headers_follow_installed_kernels() {
        let mut f = nvidia(&[0x2684]);
        f.installed.insert("linux-zen".into());
        let e = run(&f, false);
        assert!(e.packages.contains(&"linux-zen-headers".to_string()));
        assert!(!e.packages.contains(&"linux-headers".to_string()));
    }

    #[test]
    fn pre_turing_uses_nouveau() {
        let e = run(&nvidia(&[0x1b80]), true); // GTX 1080
        assert!(e.packages.contains(&"vulkan-nouveau".to_string()));
        assert!(e.packages.contains(&"lib32-vulkan-nouveau".to_string()));
        assert!(!e.packages.iter().any(|p| p.starts_with("nvidia") || p.contains("nvidia-utils")));
        assert!(e.notes.iter().any(|n| n.contains("nvidia-580xx-dkms")));
        let e = run(&nvidia(&[0x1f08]), false); // RTX 2060
        assert!(e.packages.contains(&"nvidia-open-dkms".to_string()));
        assert!(run(&nvidia(&[]), false).packages.contains(&"nvidia-open-dkms".to_string()));
    }

    #[test]
    fn flags_nvidia_open_on_a_pre_turing_gpu() {
        let mut f = nvidia(&[0x1c20]); // GTX 1060 Mobile
        f.installed.insert("nvidia-open-dkms".into());
        let e = run(&f, false);
        assert!(!e.packages.iter().any(|p| p.starts_with("nvidia")));
        assert!(e.notes.iter().any(|n| n.contains("can't drive") && n.contains("nvidia-580xx-dkms")));
        f.nvidia_devices = vec![0x2684];
        assert!(run(&f, false).notes.iter().any(|n| n.starts_with("Keeping")));
    }

    #[test]
    fn keeps_an_installed_nvidia_driver() {
        for (driver, device) in [
            ("nvidia-open", 0x2684),
            ("nvidia-dkms", 0x2684),
            ("nvidia-580xx-dkms", 0x1b80),
            ("nvidia-580xx-utils", 0x1c20),
            ("linux612-nvidia-580xx", 0x1c20),
            ("linux-cachyos-nvidia-open", 0x2684),
        ] {
            let mut f = nvidia(&[device]);
            f.installed.extend(["nvidia-utils", "nvidia-settings", "linux-firmware-nvidia", driver].map(String::from));
            let e = run(&f, true);
            assert!(
                !e.packages.iter().any(|p| p.contains("nvidia") || p.contains("nouveau")),
                "{driver}: {:?}",
                e.packages
            );
            assert!(e.notes.iter().any(|n| n.contains(driver)), "{driver}");
        }
        // Userspace alone is not a driver.
        let mut f = nvidia(&[0x2684]);
        f.installed.extend(["nvidia-utils", "nvidia-settings", "nvidia-prime"].map(String::from));
        assert!(run(&f, false).packages.contains(&"nvidia-open-dkms".to_string()));
        // Hybrid laptop on an installed legacy driver still gets PRIME offload.
        let mut f = nvidia(&[0x1c8d]);
        f.gpus = vec!["intel".into(), "nvidia".into()];
        f.installed.insert("nvidia-580xx-dkms".into());
        let e = run(&f, false);
        assert!(e.packages.contains(&"nvidia-prime".to_string()));
        assert!(e.notes.iter().any(|n| n.contains("prime-run")));
    }

    #[test]
    fn derivatives_use_their_own_nvidia_tooling() {
        let mut f = nvidia(&[0x2684]);
        f.distro_id = "manjaro".into();
        f.gpus = vec!["intel".into(), "nvidia".into()];
        let e = run(&f, true);
        assert!(!e.packages.iter().any(|p| p.contains("nvidia") || p.contains("nouveau")), "{:?}", e.packages);
        assert!(e.notes.iter().any(|n| n.contains("mhwd")));
        assert!(e.packages.contains(&"vulkan-intel".to_string()));
        f.installed.insert("linux612-nvidia".into());
        assert!(run(&f, false).notes.iter().any(|n| n.contains("Keeping the installed")));
    }

    #[test]
    fn hybrid_adds_offload_tools_without_duplicates() {
        let mut f = nvidia(&[0x28e0]);
        f.gpus = vec!["intel".into(), "nvidia".into()];
        let e = run(&f, true);
        for p in ["nvidia-prime", "switcheroo-control", "vulkan-intel", "nvidia-open-dkms"] {
            assert!(e.packages.contains(&p.to_string()), "{p}");
        }
        assert_eq!(e.enable, ["switcheroo-control.service"]);
        let unique: BTreeSet<_> = e.packages.iter().collect();
        assert_eq!(unique.len(), e.packages.len());
        f.gpus = vec!["amd".into(), "intel".into()];
        let e = run(&f, true);
        assert_eq!(e.packages.iter().filter(|p| *p == "mesa").count(), 1);
        assert!(e.enable.is_empty());
    }

    #[test]
    fn package_list_covers_every_output() {
        let mut f = nvidia(&[0x1b80]);
        f.gpus = vec!["amd".into(), "intel".into(), "nvidia".into(), "virtual".into()];
        f.installed = KERNELS.iter().map(|k| k.to_string()).collect();
        for devices in [vec![0x1b80], vec![0x2684]] {
            f.nvidia_devices = devices;
            for p in run(&f, true).packages.iter().chain(&kernel_headers(&Facts::default())) {
                assert!(PACKAGES.contains(&p.as_str()), "{p} missing from gpu::PACKAGES");
            }
        }
    }
}
