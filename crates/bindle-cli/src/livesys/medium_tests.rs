use super::*;

/// The internal disk every fixture has, never the medium.
const NVME: &str = r#"{"name":"nvme0n1","kname":"nvme0n1","path":"/dev/nvme0n1","uuid":null,"partuuid":null,"label":null,"type":"disk",
    "children":[{"name":"nvme0n1p1","kname":"nvme0n1p1","path":"/dev/nvme0n1p1","uuid":"AB12-CD34","partuuid":"1111","label":null,"type":"part"}]}"#;

fn tree(rest: &str) -> String {
    format!(r#"{{"blockdevices":[{NVME},{rest}]}}"#)
}

/// A USB stick with the ISO written to it (hybrid image: iso9660 on the
/// whole disk and its first partition, the EFI image second).
const USB: &str = r#"{"name":"sda","kname":"sda","path":"/dev/sda","uuid":"2024-11-01-10-00-00-00","partuuid":null,"label":"ARCH_202411","type":"disk",
    "children":[
      {"name":"sda1","kname":"sda1","path":"/dev/sda1","uuid":"2024-11-01-10-00-00-00","partuuid":"aaaa-01","label":"ARCH_202411","type":"part"},
      {"name":"sda2","kname":"sda2","path":"/dev/sda2","uuid":"1234-ABCD","partuuid":"aaaa-02","label":"ARCHISO_EFI","type":"part"}]}"#;

#[test]
fn plain_usb_boot() {
    let cmdline = "BOOT_IMAGE=/arch/boot/x86_64/vmlinuz-linux archisobasedir=arch archisosearchuuid=2024-11-01-10-00-00-00 cow_spacesize=4G";
    let specs = boot_specs(cmdline);
    assert_eq!(specs, [Spec::Uuid("2024-11-01-10-00-00-00".into())]);
    assert!(archiso_cmdline(cmdline));
    let m = medium(true, &specs, &tree(USB));
    assert_eq!(m, Medium::Disks(vec!["/dev/sda".into()]));
    assert!(m.contains("/dev/sda") && !m.contains("/dev/nvme0n1"));
    // The older label form finds the same disk.
    assert_eq!(medium_disks(&boot_specs("archisobasedir=arch archisolabel=ARCH_202411"), &tree(USB)), ["/dev/sda"]);
}

#[test]
fn copytoram_still_finds_the_stick() {
    // Nothing is mounted from the stick, but it is still plugged in and the
    // command line still names it.
    let cmdline = "archisobasedir=arch archisosearchuuid=2024-11-01-10-00-00-00 copytoram";
    assert_eq!(medium(true, &boot_specs(cmdline), &tree(USB)), Medium::Disks(vec!["/dev/sda".into()]));
    // Pulled out after copytoram: named but nowhere to be found.
    assert_eq!(medium(true, &boot_specs(cmdline), &format!(r#"{{"blockdevices":[{NVME}]}}"#)), Medium::Unknown);
}

#[test]
fn iso_file_on_another_disk() {
    // GRUB loop-boots archlinux.iso from the second SATA disk's data partition.
    let sdb = r#"{"name":"sdb","kname":"sdb","path":"/dev/sdb","uuid":null,"partuuid":null,"label":null,"type":"disk",
        "children":[{"name":"sdb1","kname":"sdb1","path":"/dev/sdb1","uuid":"5f1c-data","partuuid":"bbbb-01","label":"DATA","type":"part"}]}"#;
    let loop0 = r#"{"name":"loop0","kname":"loop0","path":"/dev/loop0","uuid":"2024-11-01-10-00-00-00","partuuid":null,"label":"ARCH_202411","type":"loop"}"#;
    let all = tree(&format!("{sdb},{loop0}"));
    let cmdline = "img_dev=/dev/disk/by-uuid/5f1c-data img_loop=/isos/archlinux.iso archisobasedir=arch archisosearchuuid=2024-11-01-10-00-00-00";
    let specs = boot_specs(cmdline);
    assert!(specs.contains(&Spec::Uuid("5f1c-data".into())));
    // The loop device carries the ISO's UUID too, but it isn't a disk.
    assert_eq!(medium(true, &specs, &all), Medium::Disks(vec!["/dev/sdb".into()]));
    assert_eq!(boot_specs("img_dev=/dev/sdb1"), [Spec::Path("/dev/sdb1".into())]);
    assert_eq!(medium_disks(&boot_specs("img_dev=/dev/sdb1"), &all), ["/dev/sdb"]);
}

#[test]
fn ventoy_device_mapper_source() {
    // Ventoy maps the ISO file on its exFAT partition to /dev/mapper/ventoy;
    // lsblk shows that mapping under the partition.
    let ventoy = r#"{"name":"sdc","kname":"sdc","path":"/dev/sdc","uuid":null,"partuuid":null,"label":null,"type":"disk",
        "children":[
          {"name":"sdc1","kname":"sdc1","path":"/dev/sdc1","uuid":"4E21-0000","partuuid":"cccc-01","label":"Ventoy","type":"part",
           "children":[{"name":"ventoy","kname":"dm-0","path":"/dev/mapper/ventoy","uuid":"2024-11-01-10-00-00-00","partuuid":null,"label":"ARCH_202411","type":"dm"}]},
          {"name":"sdc2","kname":"sdc2","path":"/dev/sdc2","uuid":"4E21-1111","partuuid":"cccc-02","label":"VTOYEFI","type":"part"}]}"#;
    let all = tree(ventoy);
    for cmdline in [
        "archisobasedir=arch archisolabel=ARCH_202411",
        "archisobasedir=arch archisodevice=/dev/mapper/ventoy",
        // What findmnt reports for bootmnt, resolved to the kernel name.
        "archisobasedir=arch archisodevice=/dev/dm-0",
    ] {
        assert_eq!(medium(true, &boot_specs(cmdline), &all), Medium::Disks(vec!["/dev/sdc".into()]), "{cmdline}");
    }
}

#[test]
fn not_live_and_odd_input() {
    assert_eq!(medium(false, &[], "{}"), Medium::NotLive);
    assert!(!archiso_cmdline("BOOT_IMAGE=/vmlinuz-linux root=UUID=abc rw"));
    assert_eq!(spec("LABEL=MY\\x20USB"), Some(Spec::Label("MY USB".into())));
    assert_eq!(spec("PARTUUID=bbbb-01"), Some(Spec::PartUuid("bbbb-01".into())));
    assert_eq!(spec("archlinux.iso"), None);
    assert!(medium_disks(&[Spec::Label(String::new())], &tree(USB)).is_empty());
    assert!(medium_disks(&[Spec::Uuid("x".into())], "not json").is_empty());
}

#[test]
fn open_luks_mappings_on_a_disk() {
    let json = r#"{"blockdevices":[{"name":"nvme0n1","type":"disk","children":[
        {"name":"nvme0n1p1","type":"part"},
        {"name":"nvme0n1p2","type":"part","children":[{"name":"ainstnvme0n1p2","type":"crypt"}]}]}]}"#;
    assert_eq!(crypt_mappings(json), ["ainstnvme0n1p2"]);
    assert!(crypt_mappings(&tree(USB)).is_empty());
}
