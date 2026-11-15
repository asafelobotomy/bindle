//! Unit tests for the gpu builtin.
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
fn legacy_branch_is_maxwell_to_volta() {
    assert!(nvidia_legacy(&nvidia(&[0x1b80]))); // GTX 1080
    assert!(nvidia_legacy(&nvidia(&[0x1380]))); // GTX 750 Ti
    assert!(!nvidia_legacy(&nvidia(&[0x1f08]))); // RTX 2060: nvidia-open
    assert!(!nvidia_legacy(&nvidia(&[0x1184]))); // GTX 770 (Kepler)
    assert!(!nvidia_legacy(&nvidia(&[]))); // unknown card
    assert!(!nvidia_legacy(&Facts { nvidia_devices: vec![0x1b80], ..Default::default() }));
    let mut f = nvidia(&[0x1b80]);
    f.installed.insert("linux-lts".into());
    let e = expand("nvidia-legacy", &Ctx { facts: &f, multilib: true });
    assert_eq!(e.packages, ["linux-lts-headers"]);
    assert_eq!(e.aur, ["lib32-nvidia-580xx-utils"]);
    assert!(expand("nvidia-legacy", &Ctx { facts: &f, multilib: false }).aur.is_empty());
    assert!(run(&f, false).notes.iter().any(|n| n.contains("NVIDIA driver for older cards")));
}

#[test]
fn intel_video_decoding_follows_the_generation() {
    let intel = |ids: &[u16]| {
        let mut f = Facts { distro_id: "arch".into(), gpus: vec!["intel".into()], ..Default::default() };
        f.extras.intel_devices = ids.to_vec();
        run(&f, false).packages
    };
    let haswell = intel(&[0x0416]);
    assert!(haswell.contains(&"libva-intel-driver".to_string()) && !haswell.contains(&"vpl-gpu-rt".to_string()));
    let tiger = intel(&[0x9a49]);
    assert!(tiger.contains(&"vpl-gpu-rt".to_string()) && !tiger.contains(&"libva-intel-driver".to_string()));
}

#[test]
fn hybrid_laptops_power_the_nvidia_gpu_off() {
    let laptop = |ids: &[u16]| {
        let mut f = nvidia(ids);
        f.gpus.insert(0, "intel".into());
        f.laptop = true;
        run(&f, false)
    };
    let turing = laptop(&[0x1f95]); // GTX 1650 Mobile
    assert!(turing.files.iter().any(|x| x.path == NVIDIA_PM) && turing.rebuild_initramfs);
    assert!(!turing.enable.contains(&"nvidia-powerd.service".to_string()));
    let ampere = laptop(&[0x2520]); // RTX 3060 Mobile
    assert!(ampere.enable.contains(&"nvidia-powerd.service".to_string()));
    assert!(!ampere.files.iter().any(|x| x.path == NVIDIA_PM), "Ampere does this by default");
    let mut desktop = nvidia(&[0x2520]);
    desktop.gpus.insert(0, "intel".into());
    assert!(!run(&desktop, false).enable.contains(&"nvidia-powerd.service".to_string()));
}

#[test]
fn the_580_driver_keeps_video_memory_over_suspend() {
    let e = expand("nvidia-legacy", &Ctx { facts: &nvidia(&[0x1b80]), multilib: false });
    assert!(e.enable.contains(&"nvidia-suspend.service".to_string()) && e.enable.len() == 3);
}

#[test]
fn pre_turing_uses_nouveau() {
    let e = run(&nvidia(&[0x1b80]), true); // GTX 1080
    assert!(e.packages.contains(&"vulkan-nouveau".to_string()));
    assert!(e.packages.contains(&"lib32-vulkan-nouveau".to_string()));
    assert!(!e.packages.iter().any(|p| p.starts_with("nvidia") || p.contains("nvidia-utils")));
    assert!(e.notes.iter().any(|n| n.contains("NVIDIA driver for older cards")));
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
