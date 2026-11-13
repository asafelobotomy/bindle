use super::*;

fn drive(device: &str, uuid: &str, fstype: &str, label: &str) -> Drive {
    Drive {
        device: device.into(),
        uuid: uuid.into(),
        fstype: fstype.into(),
        label: label.into(),
        size: 1_000_203_091_968,
        ..Default::default()
    }
}

fn facts(drives: Vec<Drive>) -> Facts {
    Facts { drives, uid: 1000, gid: 984, ..Default::default() }
}

fn line(d: Drive) -> String {
    let f = facts(vec![d]);
    entry(&f.drives[0], &f).unwrap().line()
}

#[test]
fn each_filesystem_gets_the_options_it_needs() {
    const COMMON: &str = "nofail,x-systemd.device-timeout=30s";
    assert_eq!(
        line(drive("/dev/sdb1", "aa-1", "ext4", "Data")),
        format!("UUID=aa-1\t/mnt/Data\text4\tdefaults,{COMMON}\t0 2")
    );
    assert_eq!(
        line(drive("/dev/sdb1", "aa-1", "btrfs", "Storage")),
        format!("UUID=aa-1\t/mnt/Storage\tbtrfs\tdefaults,noatime,compress=zstd,{COMMON}\t0 0")
    );
    // NTFS goes through the kernel's ntfs3 and belongs to the user, as do exFAT and FAT.
    assert_eq!(
        line(drive("/dev/sdb1", "01D9AB", "ntfs", "Windows")),
        format!("UUID=01D9AB\t/mnt/Windows\tntfs3\tuid=1000,gid=984,umask=022,windows_names,{COMMON}\t0 0")
    );
    assert!(
        line(drive("/dev/sdb1", "1A2B-3C4D", "exfat", "SD")).contains("\texfat\tuid=1000,gid=984,umask=022,nofail")
    );
    assert!(
        line(drive("/dev/sdb1", "1A2B-3C4D", "vfat", "USB")).contains("\tvfat\tuid=1000,gid=984,umask=022,utf8,nofail")
    );
    assert!(line(drive("/dev/sdb1", "aa-1", "xfs", "x")).ends_with(&format!("\txfs\tdefaults,{COMMON}\t0 0")));
    assert!(line(drive("/dev/sdb1", "aa-1", "ext3", "x")).contains("\text3\t"));
    for fs in crate::detect::MOUNTABLE {
        let f = facts(vec![drive("/dev/sdb1", "aa-1", fs, "Any")]);
        let e = entry(&f.drives[0], &f).unwrap();
        assert!(e.check().is_ok() && e.options.contains("nofail") && e.mount_now, "{fs}: {e:?}");
    }
}

#[test]
fn a_removable_drive_mounts_on_first_use_not_at_boot() {
    let usb = Drive { removable: true, ..drive("/dev/sdc1", "1A2B-3C4D", "exfat", "Backup") };
    let f = facts(vec![usb]);
    let e = entry(&f.drives[0], &f).unwrap();
    assert!(
        e.options
            .ends_with("nofail,x-systemd.device-timeout=30s,noauto,x-systemd.automount,x-systemd.idle-timeout=5min")
    );
    assert!(!e.mount_now && e.check().is_ok());
    let m = &modules(&f)[0];
    assert!(m.description.contains("Opens at /mnt/Backup whenever it is plugged in"), "{}", m.description);
    assert!(m.notes[0].starts_with("Removable"));
}

#[test]
fn mount_points_come_from_labels_and_never_clash() {
    let mut f = facts(vec![
        drive("/dev/sdb1", "aaaa", "ext4", "Data"),
        drive("/dev/sdc1", "bbbb", "ext4", "Data"),
        drive("/dev/sdd1", "CCCC-1234-ffff", "ext4", ""),
        drive("/dev/sde1", "dddd", "ntfs", "My Games & Stuff"),
        drive("/dev/sdf1", "eeee", "ext4", "  ../.. "),
        drive("/dev/sdg1", "ffff", "ext4", "Media"),
    ]);
    f.fstab_dirs = vec!["/".into(), "/mnt/Media".into()];
    let dir = |f: &Facts, i: usize| entry(&f.drives[i], f).unwrap().dir;
    let all: Vec<String> = (0..6).map(|i| dir(&f, i)).collect();
    assert_eq!(
        all,
        ["/mnt/Data", "/mnt/Data-2", "/mnt/drive-cccc1234", "/mnt/My_Games___Stuff", "/mnt/drive-eeee", "/mnt/Media-2"]
    );
    for i in 0..6 {
        assert!(entry(&f.drives[i], &f).unwrap().check().is_ok());
    }
    // Once in Bindle's block a drive keeps its mount point, even after a
    // relabel, and a newcomer with that name moves aside.
    f.drives[1].label = "Renamed".into();
    f.drives[1].fstab_dir = Some("/mnt/Data".into());
    f.drives[1].managed = true;
    f.fstab_dirs.push("/mnt/Data".into());
    assert_eq!((dir(&f, 1), dir(&f, 0)), ("/mnt/Data".to_string(), "/mnt/Data-2".to_string()));
}

#[test]
fn one_module_per_drive_says_what_it_is_and_where_it_mounts() {
    let hdd =
        Drive { rotational: true, model: "ST1000LM048".into(), ..drive("/dev/sda1", "179DFC88", "btrfs", "Storage") };
    let busy =
        Drive { mounted_at: vec!["/run/media/sam/Win".into()], ..drive("/dev/nvme0n1p3", "01d9", "ntfs", "Win") };
    let locked = Drive {
        blocked: Some("encrypted with LUKS: needs a key".into()),
        ..drive("/dev/sdb1", "luks-1", "crypto_LUKS", "")
    };
    let f = facts(vec![hdd, busy, locked]);
    let m = modules(&f);
    assert_eq!(m[0].id, "mount-179dfc88");
    assert_eq!(m[0].name, "Mount Storage (932 GiB Btrfs)");
    assert_eq!(
        m[0].description,
        "/dev/sda1 on a hard disk (ST1000LM048). Mounted at /mnt/Storage every time the computer starts."
    );
    assert_eq!((m[0].category.as_str(), m[0].group.as_str()), (CATEGORY, GROUP));
    assert!(m[0].notes[0].contains("default subvolume"));
    assert!(m[1].notes.iter().any(|n| n.contains("Fast Startup")));
    assert!(
        m[1].notes
            .iter()
            .any(|n| n == "Mounted at /run/media/sam/Win right now; it is at /mnt/Win from the next start.")
    );
    assert!(!entry(&f.drives[1], &f).unwrap().mount_now);
    // An encrypted volume is listed but can't be ticked, and has no line.
    assert_eq!(m[2].name, "Mount unnamed drive (932 GiB LUKS-encrypted)");
    assert_eq!(m[2].description, "/dev/sdb1 on an SSD.");
    assert_eq!(f.matches(&m[2].when).unwrap_err(), "encrypted with LUKS: needs a key");
    assert!(f.matches(&m[0].when).is_ok() && entry(&f.drives[2], &f).is_none());
    assert_eq!(Facts::default().matches(&m[0].when).unwrap_err(), "that drive is not connected");
    // The builtin expands to exactly that drive's line.
    let e = super::super::expand(m[0].builtin.as_deref().unwrap(), &super::super::Ctx { facts: &f, multilib: false });
    assert_eq!(e.mounts, [entry(&f.drives[0], &f).unwrap()]);
    assert!(e.packages.is_empty() && e.files.is_empty());
}

#[test]
fn sizes_read_naturally() {
    assert_eq!(size_label(1_000_203_091_968), "932 GiB");
    assert_eq!(size_label(4 * (1 << 40)), "4.0 TiB");
    assert_eq!(size_label(64_000_000_000), "60 GiB");
    assert_eq!(size_label(536_870_912), "512 MiB");
}
