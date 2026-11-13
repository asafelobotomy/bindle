use super::*;

/// One block device as lsblk prints it; `extra` holds the remaining JSON fields.
fn node(path: &str, kind: &str, fstype: &str, uuid: &str, label: &str, mounts: &[&str], extra: &str) -> String {
    let quoted = |s: &str| if s.is_empty() { "null".to_string() } else { format!("\"{s}\"") };
    let mounts: Vec<String> = mounts.iter().map(|m| format!("\"{m}\"")).collect();
    format!(
        r#"{{"path":"{path}","type":"{kind}","fstype":{},"uuid":{},"label":{},"size":1000203091968,
            "mountpoints":[{}]{}{extra}}}"#,
        quoted(fstype),
        quoted(uuid),
        quoted(label),
        mounts.join(","),
        if extra.is_empty() { "" } else { "," },
    )
}

fn disk(path: &str, extra: &str, children: &[String]) -> String {
    let extra = format!(r#"{extra}{}"children":[{}]"#, if extra.is_empty() { "" } else { "," }, children.join(","));
    node(path, "disk", "", "", "", &[], &extra)
}

fn lsblk(disks: &[String]) -> String {
    format!(r#"{{"blockdevices":[{}]}}"#, disks.join(","))
}

const ESP: &str = r#""parttype":"c12a7328-f81f-11d2-ba4b-00a0c93ec93b""#;
const HDD: &str = r#""rota":true,"rm":false,"hotplug":false,"tran":"sata","model":"ST1000LM048 ""#;

/// An NVMe system disk (ESP, btrfs root, a Windows partition beside them), a
/// SATA data disk and zram swap: a typical dual-boot desktop.
fn desktop() -> String {
    lsblk(&[
        disk(
            "/dev/nvme0n1",
            r#""rota":false,"tran":"nvme""#,
            &[
                node("/dev/nvme0n1p1", "part", "vfat", "AC7B-1007", "", &["/boot"], ESP),
                node("/dev/nvme0n1p2", "part", "btrfs", "root-uuid", "", &["/var/log", "/home", "/"], ""),
                node("/dev/nvme0n1p3", "part", "ntfs", "01D9ABCDEF012345", "Windows", &[], ""),
            ],
        ),
        disk("/dev/sda", HDD, &[node("/dev/sda1", "part", "btrfs", "179dfc88", "Storage", &[], "")]),
        node("/dev/zram0", "disk", "swap", "4e4daaba", "zram0", &["[SWAP]"], ""),
    ])
}

#[test]
fn offers_data_filesystems_and_leaves_out_the_systems_own() {
    let drives = drives_from(&desktop(), "UUID=root-uuid / btrfs rw 0 0\nUUID=AC7B-1007 /boot vfat rw 0 2\n");
    let devices: Vec<&str> = drives.iter().map(|d| d.device.as_str()).collect();
    assert_eq!(devices, ["/dev/nvme0n1p3", "/dev/sda1"]);
    let win = &drives[0];
    assert_eq!((win.fstype.as_str(), win.label.as_str(), win.rotational), ("ntfs", "Windows", false));
    assert_eq!(win.key(), "01d9abcdef012345");
    let storage = &drives[1];
    assert_eq!(storage.size, 1_000_203_091_968);
    assert!(storage.rotational && !storage.removable && storage.blocked.is_none());
    assert_eq!(storage.model, "ST1000LM048");
    assert!(drives_from("not json", "").is_empty());
}

#[test]
fn firmware_recovery_and_unsupported_filesystems_are_never_offered() {
    let out = lsblk(&[disk(
        "/dev/sdb",
        "",
        &[
            node("/dev/sdb1", "part", "vfat", "1111-2222", "EFI", &[], ESP),
            node(
                "/dev/sdb2",
                "part",
                "ntfs",
                "aaaa",
                "Recovery",
                &[],
                r#""parttype":"DE94BBA4-06D1-4D40-A16A-BFD50179D6AC""#,
            ),
            node("/dev/sdb3", "part", "swap", "bbbb", "", &[], ""),
            node("/dev/sdb4", "part", "LVM2_member", "cccc", "", &[], ""),
            node("/dev/sdb5", "part", "ext4", "", "no-uuid", &[], ""),
            node("/dev/sdb6", "part", "ext4", "dddd", "Data", &[], ""),
        ],
    )]);
    let drives = drives_from(&out, "");
    assert_eq!(drives.iter().map(|d| d.device.as_str()).collect::<Vec<_>>(), ["/dev/sdb6"]);
    // The live ISO's own stick and loop images are in use, wherever they are mounted.
    let iso = lsblk(&[
        disk("/dev/sdc", "", &[node("/dev/sdc1", "part", "vfat", "eeee", "ARCH", &["/run/archiso/bootmnt"], "")]),
        node("/dev/loop0", "loop", "ext4", "ffff", "", &[], ""),
    ]);
    assert!(drives_from(&iso, "").is_empty());
}

#[test]
fn removable_and_already_mounted_drives_are_described() {
    let out = lsblk(&[
        disk(
            "/dev/sdd",
            r#""rota":false,"rm":false,"hotplug":true,"tran":"usb""#,
            &[node("/dev/sdd1", "part", "exfat", "1A2B-3C4D", "Backup", &["/run/media/sam/Backup"], "")],
        ),
        // Old lsblk prints flags and sizes as strings.
        node("/dev/sde", "disk", "ext4", "whole-disk", "", &["/mnt/old"], r#""rm":"1","rota":"1""#),
    ]);
    let drives = drives_from(&out, "");
    assert!(drives[0].removable && !drives[0].rotational);
    assert_eq!(drives[0].mounted_at, ["/run/media/sam/Backup"]);
    assert!(drives[1].removable && drives[1].rotational, "a filesystem on the whole disk counts too");
}

#[test]
fn encrypted_volumes_are_listed_with_the_reason_they_are_not_offered() {
    let inner = node("/dev/mapper/vault", "crypt", "ext4", "inner", "Vault", &[], "");
    let root = node("/dev/mapper/root", "crypt", "btrfs", "rootfs", "", &["/"], "");
    let out = lsblk(&[disk(
        "/dev/sdf",
        "",
        &[
            node("/dev/sdf1", "part", "crypto_LUKS", "luks-1", "", &[], &format!(r#""children":[{inner}]"#)),
            node("/dev/sdf2", "part", "BitLocker", "", "", &[], ""),
            node("/dev/sdf3", "part", "crypto_LUKS", "luks-root", "", &[], &format!(r#""children":[{root}]"#)),
        ],
    )]);
    let drives = drives_from(&out, "");
    assert_eq!(drives.iter().map(|d| d.device.as_str()).collect::<Vec<_>>(), ["/dev/sdf1", "/dev/sdf2"]);
    assert!(drives[0].blocked.as_deref().unwrap().starts_with("encrypted with LUKS"));
    assert!(drives[1].blocked.as_deref().unwrap().starts_with("encrypted with BitLocker"));
    assert_eq!(drives[1].key(), "sdf2");
}

#[test]
fn a_multi_device_filesystem_is_one_drive_and_follows_the_system() {
    let pool = |mounts: &[&str]| {
        lsblk(&[
            disk("/dev/sdg", "", &[node("/dev/sdg1", "part", "btrfs", "pool", "Pool", mounts, "")]),
            disk("/dev/sdh", "", &[node("/dev/sdh1", "part", "btrfs", "pool", "Pool", &[], "")]),
        ])
    };
    assert_eq!(drives_from(&pool(&[]), "").len(), 1);
    assert!(drives_from(&pool(&["/"]), "").is_empty(), "the second member of the root filesystem is not a data drive");
}

#[test]
fn fstab_lines_are_matched_by_uuid_device_label_and_partuuid() {
    let out = lsblk(&[disk(
        "/dev/sdi",
        "",
        &[
            node("/dev/sdi1", "part", "ext4", "AAAA", "one", &[], ""),
            node("/dev/sdi2", "part", "ext4", "bbbb", "two", &[], ""),
            node("/dev/sdi3", "part", "ext4", "cccc", "three", &[], ""),
            node("/dev/sdi4", "part", "ext4", "dddd", "four", &[], r#""partuuid":"P-4""#),
            node("/dev/sdi5", "part", "ext4", "eeee", "five", &[], ""),
            node("/dev/sdi6", "part", "ext4", "ffff", "six", &[], ""),
        ],
    )]);
    let fstab = format!(
        "UUID=aaaa /data ext4 defaults 0 2\n/dev/sdi2 /two ext4 noauto 0 0\nLABEL=three /three ext4 defaults 0 0\n\
         PARTUUID=p-4 /four ext4 defaults 0 0\n{}\nUUID=eeee\t/mnt/five\text4\tdefaults,nofail\t0 2\n{}\n",
        fstab::BEGIN,
        fstab::END
    );
    let drives = drives_from(&out, &fstab);
    for (d, dir) in drives.iter().zip(["/data", "/two", "/three", "/four"]) {
        assert_eq!(d.blocked.as_deref(), Some(format!("already in /etc/fstab, mounted at {dir}").as_str()));
    }
    // Bindle's own line: still offered, and it keeps its mount point.
    assert_eq!(
        (drives[4].fstab_dir.as_deref(), drives[4].managed, &drives[4].blocked),
        (Some("/mnt/five"), true, &None)
    );
    assert_eq!((&drives[5].fstab_dir, &drives[5].blocked), (&None, &None));
}
