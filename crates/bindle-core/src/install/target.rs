//! The complete archinstall configuration for `bindle install`: Bindle's
//! hardware-aware preset plus the answers archinstall would otherwise ask for
//! (disk, filesystem, encryption, bootloader, users, locale), so archinstall
//! can run unattended (`--silent`). Pure: the wizard collects, this builds.

use serde_json::{Value, json};

use super::hardware::Hardware;
use super::preset::{self, IsoChoice};
use crate::desktops::Desktop;
use crate::detect::Facts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filesystem {
    /// Btrfs with archinstall's subvolumes and Snapper snapshots.
    Btrfs,
    Ext4,
}

#[derive(Debug, Clone)]
pub struct Answers {
    pub disk: String,
    pub disk_bytes: u64,
    pub filesystem: Filesystem,
    /// Encrypt the root partition with this passphrase (LUKS).
    pub encryption: Option<String>,
    pub uefi: bool,
    pub hostname: String,
    pub username: String,
    pub password: String,
    pub timezone: String,
    pub keyboard: String,
    pub locale: String,
    /// The Hardware page's rows (kernel, boot loader, drivers, swap…).
    pub hardware: Hardware,
}

const MIB: u64 = 1024 * 1024;
const ESP_MIB: u64 = 1024;
/// Smallest disk the default layout accepts.
pub const MIN_DISK_BYTES: u64 = 20 * 1024 * MIB;

const ROOT_ID: &str = "b1d1e000-0000-4000-8000-000000000002";
const ESP_ID: &str = "b1d1e000-0000-4000-8000-000000000001";

fn mib(v: u64) -> Value {
    json!({ "value": v, "unit": "MiB", "sector_size": { "value": 512, "unit": "B" } })
}

fn partition(id: &str, fs: &str, start: u64, size: u64, mount: Option<&str>) -> Value {
    json!({
        "obj_id": id, "status": "create", "type": "primary", "fs_type": fs, "flags": [],
        "start": mib(start), "size": mib(size), "mountpoint": mount,
        "mount_options": [], "dev_path": null, "btrfs": [],
    })
}

/// Whole-disk layout: a 1 GiB EFI/boot partition and root on the rest.
pub fn disk_config(a: &Answers) -> Value {
    let mut esp = partition(ESP_ID, "fat32", 1, ESP_MIB, Some("/boot"));
    // archinstall partitions GPT on UEFI and MBR on BIOS, where `esp` doesn't exist.
    esp["flags"] = if a.uefi { json!(["boot", "esp"]) } else { json!(["boot"]) };
    // Leave 1 MiB at the end for the backup GPT.
    let root_size = (a.disk_bytes / MIB).saturating_sub(1 + ESP_MIB + 1);
    let mut root = match a.filesystem {
        Filesystem::Ext4 => partition(ROOT_ID, "ext4", 1 + ESP_MIB, root_size, Some("/")),
        Filesystem::Btrfs => {
            let mut p = partition(ROOT_ID, "btrfs", 1 + ESP_MIB, root_size, None);
            p["mount_options"] = json!(["compress=zstd"]);
            p["btrfs"] = json!([
                { "name": "@", "mountpoint": "/" },
                { "name": "@home", "mountpoint": "/home" },
                { "name": "@log", "mountpoint": "/var/log" },
                { "name": "@pkg", "mountpoint": "/var/cache/pacman/pkg" },
                { "name": "@.snapshots", "mountpoint": "/.snapshots" },
            ]);
            p
        }
    };
    if a.filesystem == Filesystem::Ext4 {
        root["mount_options"] = json!([]);
    }
    let mut cfg = json!({
        "config_type": "default_layout",
        "device_modifications": [{ "device": a.disk, "wipe": true, "partitions": [esp, root] }],
    });
    if a.filesystem == Filesystem::Btrfs {
        cfg["btrfs_options"] = json!({ "snapshot_config": { "type": "Snapper" } });
    }
    if a.encryption.is_some() {
        cfg["disk_encryption"] = json!({ "encryption_type": "luks", "partitions": [ROOT_ID], "lvm_volumes": [] });
    }
    cfg
}

/// The full config: Bindle's preset for `choice`, then everything archinstall
/// would ask in its menu.
pub fn config(facts: &Facts, desktops: &[Desktop], choice: &IsoChoice, a: &Answers) -> Value {
    let mut cfg = preset::build(facts, desktops, choice, None);
    let extra = json!({
        "hostname": a.hostname,
        "timezone": a.timezone,
        "locale_config": { "kb_layout": a.keyboard, "sys_enc": "UTF-8", "sys_lang": a.locale },
        "disk_config": disk_config(a),
    });
    preset::merge(&mut cfg, extra);
    a.hardware.apply(&mut cfg);
    cfg
}

/// archinstall's credentials file: the user (with sudo) and the disk passphrase.
pub fn creds(a: &Answers) -> Value {
    let mut c = json!({ "users": [{ "username": a.username, "!password": a.password, "sudo": true }] });
    if let Some(p) = &a.encryption {
        c["encryption_password"] = json!(p);
    }
    c
}

/// A hostname archinstall and systemd accept: 1–63 of a-z, 0-9 and `-`,
/// not starting or ending with `-`.
pub fn valid_hostname(h: &str) -> bool {
    (1..=63).contains(&h.len())
        && h.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !h.starts_with('-')
        && !h.ends_with('-')
}

/// A login name useradd accepts by default (lower case, starts with a letter
/// or `_`), not one the base system already has.
pub fn valid_username(u: &str) -> bool {
    const TAKEN: &[&str] = &["root", "bin", "daemon", "mail", "ftp", "http", "nobody", "dbus", "polkitd"];
    let mut chars = u.chars();
    (1..=32).contains(&u.len())
        && chars.next().is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        && !TAKEN.contains(&u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktops;

    fn answers() -> Answers {
        Answers {
            disk: "/dev/nvme0n1".into(),
            disk_bytes: 512 * 1024 * MIB,
            filesystem: Filesystem::Btrfs,
            encryption: None,
            uefi: true,
            hostname: "laptop".into(),
            username: "solon".into(),
            password: "pw".into(),
            timezone: "Europe/London".into(),
            keyboard: "uk".into(),
            locale: "en_GB".into(),
            hardware: Hardware::recommended(&Facts::default(), true, true),
        }
    }

    #[test]
    fn btrfs_layout_with_snapper() {
        let d = disk_config(&answers());
        let parts = d.pointer("/device_modifications/0/partitions").unwrap().as_array().unwrap();
        assert_eq!(parts[0]["mountpoint"], "/boot");
        assert_eq!(parts[0]["flags"], json!(["boot", "esp"]));
        assert_eq!(parts[1]["fs_type"], "btrfs");
        assert!(parts[1]["mountpoint"].is_null());
        assert_eq!(parts[1]["btrfs"][0], json!({ "name": "@", "mountpoint": "/" }));
        assert_eq!(parts[1]["size"]["value"], 512 * 1024 - 1026);
        assert_eq!(d["btrfs_options"]["snapshot_config"]["type"], "Snapper");
        assert!(d.get("disk_encryption").is_none());
        let bios = disk_config(&Answers { uefi: false, ..answers() });
        assert_eq!(bios.pointer("/device_modifications/0/partitions/0/flags").unwrap(), &json!(["boot"]));
    }

    #[test]
    fn ext4_and_encryption() {
        let mut a = answers();
        a.filesystem = Filesystem::Ext4;
        a.encryption = Some("secret".into());
        let d = disk_config(&a);
        assert_eq!(d.pointer("/device_modifications/0/partitions/1/mountpoint").unwrap(), "/");
        assert!(d.get("btrfs_options").is_none());
        assert_eq!(d["disk_encryption"]["partitions"], json!([ROOT_ID]));
        assert_eq!(creds(&a)["encryption_password"], "secret");
    }

    #[test]
    fn full_config_keeps_the_preset() {
        let data = desktops::all().unwrap();
        let choice = IsoChoice { desktop: Some("uq".into()), profile: "desktop".into() };
        let mut a = answers();
        let cfg = config(&Facts::default(), &data, &choice, &a);
        assert_eq!(cfg["hostname"], "laptop");
        assert_eq!(cfg["network_config"]["type"], "nm");
        assert_eq!(cfg.pointer("/profile_config/profile/details/0").unwrap(), "Hyprland");
        assert_eq!(cfg["bootloader_config"]["bootloader"], "Systemd-boot");
        assert_eq!(cfg["locale_config"]["kb_layout"], "uk");
        a.uefi = false;
        a.hardware = Hardware::recommended(&Facts::default(), false, true);
        assert_eq!(config(&Facts::default(), &data, &choice, &a)["bootloader_config"]["bootloader"], "Grub");
        let c = creds(&a);
        assert_eq!(c["users"][0], json!({ "username": "solon", "!password": "pw", "sudo": true }));
        assert!(c.get("encryption_password").is_none());
    }

    #[test]
    fn names() {
        assert!(valid_hostname("my-laptop2"));
        assert!(!valid_hostname("-x") && !valid_hostname("My") && !valid_hostname(""));
        assert!(valid_username("solon") && valid_username("_svc") && valid_username("a-b_1"));
        assert!(!valid_username("root") && !valid_username("1x") && !valid_username("Solon"));
    }
}
