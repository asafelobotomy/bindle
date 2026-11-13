use super::*;
use std::cmp::Ordering::{Equal, Greater, Less};

#[test]
fn free_space_per_mount() {
    let text = "Mounted on Type Avail\n/ btrfs 41234M\n/boot vfat 87M\n/boot vfat 87M\n";
    let m = df(text);
    assert_eq!(m.len(), 2);
    assert_eq!(m[1], Mount { target: "/boot".into(), fstype: "vfat".into(), avail_mb: 87 });
    assert_eq!(efi(&m).unwrap().target, "/boot", "a FAT /boot is the EFI partition");
    assert!(efi(&df("h\n/ ext4 9M\n/boot ext4 9M\n")).is_none());
}

#[test]
fn dkms_status_lines() {
    let d = dkms(
        "nvidia/580.196.07, 7.3.4-zen2-1-zen, x86_64: installed\n\
         nvidia, 580, 6.1.1-arch1-1, x86_64: installed (original_module exists)\n\
         v4l2loopback/0.13: added\n",
    );
    assert_eq!(d[0], Dkms { module: "nvidia".into(), kernel: "7.3.4-zen2-1-zen".into(), state: "installed".into() });
    assert_eq!(d[1].kernel, "6.1.1-arch1-1");
    assert_eq!(d[1].state, "installed");
    assert_eq!(d[2], Dkms { module: "v4l2loopback".into(), kernel: String::new(), state: "added".into() });
}

#[test]
fn modules_must_be_built_for_every_kernel() {
    let kernels = [
        Kernel { version: "7.3.4-zen2-1-zen".into(), pkgbase: "linux-zen".into() },
        Kernel { version: "7.3.5-zen1-1-zen".into(), pkgbase: "linux-zen".into() },
    ];
    let entries = dkms("nvidia/580, 7.3.4-zen2-1-zen, x86_64: installed\n");
    assert_eq!(dkms_missing(&kernels, &entries), [("nvidia".to_string(), "7.3.5-zen1-1-zen".to_string())]);
    assert!(dkms_missing(&kernels[..1], &entries).is_empty());
    assert!(dkms_missing(&kernels, &[]).is_empty(), "no DKMS modules, nothing to build");
}

#[test]
fn boot_images_for_each_kernel() {
    let kernels = [Kernel { version: "7.3.4".into(), pkgbase: "linux-zen".into() }];
    let boot = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(images_missing(&kernels, &boot(&["vmlinuz-linux-zen", "initramfs-linux-zen.img"])).is_empty());
    assert_eq!(images_missing(&kernels, &boot(&["vmlinuz-linux-zen"])), ["initramfs-linux-zen.img"]);
    assert!(images_missing(&kernels, &boot(&["EFI", "loader"])).is_empty(), "unified kernel images");
}

#[test]
fn failed_unit_names() {
    let text = "● bluetooth.service loaded failed failed Bluetooth service\nfoo.mount loaded failed failed /foo\n";
    assert_eq!(failed_units(text), ["bluetooth.service", "foo.mount"]);
    assert!(failed_units("").is_empty());
}

#[test]
fn rebuild_detector_lines() {
    let r = checkrebuild("foreign\tpython-foo\nextra\tbar\n");
    assert_eq!(r[0], Rebuild { repo: "foreign".into(), name: "python-foo".into() });
    assert_eq!(r[1].repo, "extra");
}

#[test]
fn battery_and_mains() {
    let bat = ("Battery".to_string(), None, Some(41));
    let ac = |on| ("Mains".to_string(), Some(on), None);
    assert_eq!(power(&[bat.clone(), ac(false)]), Some(Power { on_battery: true, capacity: Some(41) }));
    assert!(!power(&[bat.clone(), ac(true)]).unwrap().on_battery);
    assert!(power(&[bat]).unwrap().on_battery, "no mains supply at all");
    assert_eq!(power(&[ac(true)]), None, "a desktop");
}

#[test]
fn hyprland_pin_range() {
    let p = pin("hyprland>=0.56.2  hyprland<0.58.0");
    assert_eq!(p.at_least.as_deref(), Some("0.56.2"));
    assert!(p.allows("0.57.1-1") && p.allows("0.56.2-2"));
    assert!(!p.allows("0.58.0-1") && !p.allows("0.55.0-1"));
    assert!(Pin::default().allows("9.9"));
}

#[test]
fn uqctl_setup_report() {
    let json = r#"{"hyprland_lua":{"path":"/h/.config/hypr/hyprland.lua","outcome":"installed"},
        "apps":[{"path":"/h/.config/kitty/kitty.conf","outcome":"kept_user_file"}],"exit":0}"#;
    let lines = uq_setup(json).unwrap();
    assert_eq!(lines[0], "wrote /h/.config/hypr/hyprland.lua");
    assert!(lines[1].starts_with("kept /h/.config/kitty/kitty.conf"));
    assert_eq!(
        uq_setup(r#"{"error":"missing","message":"uq isn't installed","exit":1}"#).unwrap_err(),
        "uq isn't installed"
    );
    assert!(uq_setup("not json").is_err());
}

#[test]
fn version_order() {
    assert_eq!(vercmp("0.58.0", "0.57.9"), Greater);
    assert_eq!(vercmp("1.0.10", "1.0.9"), Greater);
    assert_eq!(vercmp("1.0rc1", "1.0"), Less);
    assert_eq!(vercmp("1.0", "1.0.1"), Less);
    assert_eq!(vercmp("1:0.1", "2.0"), Greater);
    assert_eq!(vercmp("0.1.8", "0.1.8"), Equal);
    assert_eq!(vercmp("007", "7"), Equal);
}

#[test]
fn metadata_refresh_age() {
    assert!(stale(None, 7) && stale(Some(9), 7));
    assert!(!stale(Some(2), 7));
}
