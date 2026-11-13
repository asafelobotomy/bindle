use super::*;
use crate::desktops;
use crate::install::preset::{self, IsoChoice};

fn laptop() -> Facts {
    Facts {
        gpus: vec!["intel".into()],
        cpu_vendor: "GenuineIntel".into(),
        laptop: true,
        bluetooth: true,
        ram_mib: 16384,
        ..Default::default()
    }
}

fn pick(hw: &mut Hardware, s: Setting, value: &str) {
    let r = hw.rows.iter_mut().find(|r| r.setting == s).unwrap();
    r.selected = r.choices.iter().position(|c| c.value == value).unwrap();
}

#[test]
fn recommendation_matches_the_preset() {
    let d = desktops::all().unwrap();
    let choice = IsoChoice { desktop: Some("uq".into()), profile: "desktop".into() };
    let before = preset::build(&laptop(), &d, &choice, None);
    let mut after = before.clone();
    let hw = Hardware::recommended(&laptop(), true, true);
    assert!(!hw.changed());
    hw.apply(&mut after);
    // Nothing the preset decided changes; kernel and boot loader are added.
    for key in ["app_config", "swap", "profile_config"] {
        assert_eq!(before[key], after[key], "{key}");
    }
    assert_eq!(after["kernels"], json!(["linux"]));
    assert_eq!(after["bootloader_config"]["bootloader"], "Systemd-boot");
    let ucode = hw.rows.iter().find(|r| r.setting == Setting::Microcode).unwrap();
    assert!(ucode.fixed() && ucode.choice().label == "intel-ucode");
}

#[test]
fn changed_rows_reach_the_config() {
    let d = desktops::all().unwrap();
    let choice = IsoChoice { desktop: Some("plasma".into()), profile: "desktop".into() };
    let mut cfg = preset::build(&laptop(), &d, &choice, None);
    let mut hw = Hardware::recommended(&laptop(), true, true);
    pick(&mut hw, Setting::Kernel, "linux-lts");
    pick(&mut hw, Setting::Bootloader, "Grub");
    pick(&mut hw, Setting::Swap, OFF);
    pick(&mut hw, Setting::Power, OFF);
    pick(&mut hw, Setting::Bluetooth, OFF);
    pick(&mut hw, Setting::Audio, "pulseaudio");
    pick(&mut hw, Setting::Graphics, GFX_DRIVERS[0].0);
    assert!(hw.changed());
    hw.apply(&mut cfg);
    assert_eq!(cfg["kernels"], json!(["linux-lts"]));
    assert_eq!(cfg["bootloader_config"]["bootloader"], "Grub");
    assert_eq!(cfg["swap"], json!({ "enabled": false }));
    assert!(cfg["app_config"].get("power_management_config").is_none());
    assert!(cfg["app_config"].get("bluetooth_config").is_none());
    assert_eq!(cfg["app_config"]["audio_config"]["audio"], "pulseaudio");
    assert_eq!(cfg["profile_config"]["gfx_driver"], "All open-source");
}

#[test]
fn the_makers_driver_is_recommended_and_open_source_optional() {
    let nvidia =
        |devices: &[u16]| Facts { gpus: vec!["nvidia".into()], nvidia_devices: devices.to_vec(), ..Default::default() };
    let gfx = |f: &Facts| {
        Hardware::recommended(f, true, true).rows.into_iter().find(|r| r.setting == Setting::Graphics).unwrap()
    };
    // RTX: nvidia-open is NVIDIA's own; nouveau is there to choose.
    let r = gfx(&nvidia(&[0x2684]));
    assert!(r.choice().label.starts_with("NVIDIA's own driver"));
    assert!(r.choices.iter().any(|c| c.label.contains("nouveau")));
    assert!(r.why.contains("recommended"));
    // GTX 1080: the 580 branch from the AUR, over archinstall's nouveau.
    let f = nvidia(&[0x1b80]);
    let mut hw = Hardware::recommended(&f, true, true);
    let r = hw.rows.iter().find(|r| r.setting == Setting::Graphics).unwrap();
    assert_eq!(r.choice().module, Some(NVIDIA_LEGACY_MODULE));
    assert_eq!(r.choice().value, "Nvidia (open-source nouveau driver)");
    assert!(hw.excluded_modules().is_empty() && !hw.changed());
    pick(&mut hw, Setting::Graphics, "All open-source");
    assert_eq!(hw.excluded_modules(), [NVIDIA_LEGACY_MODULE]);
    // Hybrid laptop with an older NVIDIA card: the iGPU's drivers too.
    let hybrid = Facts { gpus: vec!["intel".into(), "nvidia".into()], ..nvidia(&[0x1c20]) };
    assert_eq!(gfx(&hybrid).choice().value, "All open-source");
    // GTX 770 (Kepler): no branch Bindle installs, so nouveau, and it says why.
    let r = gfx(&nvidia(&[0x1184]));
    assert!(r.choice().label.contains("nouveau") && r.why.contains("no longer"));
    assert!(Hardware::recommended(&nvidia(&[0x1184]), true, true).excluded_modules().is_empty());
    // AMD: unchanged.
    assert!(gfx(&Facts { gpus: vec!["amd".into()], ..Default::default() }).choice().label.starts_with("AMD"));
}

#[test]
fn bios_and_no_desktop() {
    let hw = Hardware::recommended(&Facts::default(), false, false);
    let has = |s| hw.rows.iter().any(|r| r.setting == s);
    assert!(!has(Setting::Graphics) && !has(Setting::Power) && !has(Setting::Bluetooth));
    let boot = hw.rows.iter().find(|r| r.setting == Setting::Bootloader).unwrap();
    assert_eq!(boot.choice().value, "Grub");
    assert!(boot.choices.iter().all(|c| c.value != "Systemd-boot"));
    let d = desktops::all().unwrap();
    let mut cfg = preset::build(&Facts::default(), &d, &IsoChoice { desktop: None, profile: "minimal".into() }, None);
    hw.apply(&mut cfg);
    assert!(cfg.get("profile_config").is_none());
}

#[test]
fn rows_cycle_both_ways() {
    let mut hw = Hardware::recommended(&laptop(), true, true);
    let k = hw.rows.iter_mut().find(|r| r.setting == Setting::Kernel).unwrap();
    k.cycle(false);
    assert_eq!(k.choice().value, "linux-zen");
    k.cycle(true);
    assert_eq!(k.choice().value, "linux");
}

#[test]
fn older_nvidia_cards_get_nvidias_driver_unless_unticked() {
    use crate::plan::{Selection, Step, resolve};
    let catalog = crate::Catalog::arch().unwrap();
    let gtx1080 = Facts {
        distro_id: "arch".into(),
        gpus: vec!["nvidia".into()],
        nvidia_devices: vec![0x1b80],
        user: "tester".into(),
        ..Default::default()
    };
    let aur = |f: &Facts, sel: Selection| -> Vec<String> {
        let plan = resolve(&catalog, f, &sel, &|_| None).unwrap();
        plan.steps
            .iter()
            .find_map(|s| match s {
                Step::InstallAur { packages } => Some(packages.clone()),
                _ => None,
            })
            .unwrap_or_default()
    };
    let desktop = || Selection { profile: Some("desktop".into()), ..Default::default() };
    // Recommended: in the plan without being asked for.
    let got = aur(&gtx1080, desktop());
    assert!(
        got.contains(&"nvidia-580xx-dkms".to_string()) && got.contains(&"nvidia-580xx-utils".to_string()),
        "{got:?}"
    );
    // Open source stays a choice: untick the module (what the Hardware page does).
    let open = Selection { exclude: vec![NVIDIA_LEGACY_MODULE.into()], ..desktop() };
    assert!(!aur(&gtx1080, open).iter().any(|p| p.contains("nvidia")));
    // A current card never gets the legacy branch.
    let rtx = Facts { nvidia_devices: vec![0x2684], ..gtx1080.clone() };
    assert!(!aur(&rtx, desktop()).iter().any(|p| p.contains("580xx")));
}
