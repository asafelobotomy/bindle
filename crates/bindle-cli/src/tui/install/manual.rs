//! "Manual partitioning" on the Disk page: archinstall's own disk editor for
//! that one step. A small Python helper opens the editor from the ISO's
//! archinstall and saves the layout in archinstall's own format; nothing
//! touches a disk until Install is confirmed on Review.

use std::fs;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use bindle_core::install::target::MIN_DISK_BYTES;

use crate::apply::sys::write_atomic;
use crate::iso::private_workdir;
use crate::livesys::Medium;

const EDITOR: &str = include_str!("../../../../../packaging/archinstall/bindle_disk_editor.py");

#[derive(Debug, Clone, PartialEq)]
pub struct ManualDisk {
    /// archinstall's `disk_config`, passed to the install unchanged.
    pub config: Value,
    /// The passphrase the editor asked for, when it set up encryption.
    pub passphrase: Option<String>,
}

impl ManualDisk {
    /// What the helper wrote. None: not a layout archinstall can install.
    pub fn parse(json: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(json).ok()?;
        let config = v.get("disk_config").filter(|c| c.get("config_type").is_some_and(Value::is_string))?.clone();
        let passphrase = v.get("encryption_password").and_then(Value::as_str).filter(|p| !p.is_empty());
        Some(ManualDisk { config, passphrase: passphrase.map(String::from) })
    }

    /// One line per disk: what will happen to it.
    pub fn lines(&self) -> Vec<String> {
        if let Some(m) = self.config.get("mountpoint").and_then(Value::as_str) {
            return vec![format!("Already mounted at {m}: used as it is, nothing is partitioned")];
        }
        let mods = self.config.get("device_modifications").and_then(Value::as_array);
        let mut out: Vec<String> = mods
            .into_iter()
            .flatten()
            .map(|d| {
                let dev = d.get("device").and_then(Value::as_str).unwrap_or("?");
                let parts = d.get("partitions").and_then(Value::as_array).map_or(0, Vec::len);
                let wipe = if d.get("wipe").and_then(Value::as_bool) == Some(true) { ", erased first" } else { "" };
                format!("{dev}: {parts} partition{}{wipe}", if parts == 1 { "" } else { "s" })
            })
            .collect();
        if self.config.get("lvm_config").is_some_and(|l| !l.is_null()) {
            out.push("LVM volumes".into());
        }
        let encrypted = self.config.pointer("/disk_encryption/encryption_type").and_then(Value::as_str);
        if encrypted.is_some_and(|t| t != "no_encryption") {
            out.push("Encrypted (the passphrase you gave the editor)".into());
        }
        out
    }

    /// The disks it touches, for the "really install?" question.
    pub fn devices(&self) -> String {
        let mods = self.config.get("device_modifications").and_then(Value::as_array);
        let devs: Vec<&str> =
            mods.into_iter().flatten().filter_map(|d| d.get("device").and_then(Value::as_str)).collect();
        if devs.is_empty() { "the mounted disks".into() } else { devs.join(", ") }
    }
}

/// archinstall's size object (`{"value": 512, "unit": "MiB", "sector_size": …}`) in bytes.
fn bytes(size: &Value) -> Option<u64> {
    let value = size.get("value")?.as_u64()?;
    let unit = size.get("unit")?.as_str()?;
    let sector = size.get("sector_size").and_then(bytes).unwrap_or(512);
    let mult: u64 = match unit {
        "B" => 1,
        "sectors" => sector,
        "KiB" => 1 << 10,
        "MiB" => 1 << 20,
        "GiB" => 1 << 30,
        "TiB" => 1 << 40,
        "kB" => 1_000,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        "TB" => 1_000_000_000_000,
        _ => return None,
    };
    value.checked_mul(mult)
}

fn mounted_at(p: &Value, at: &str) -> bool {
    p.get("mountpoint").and_then(Value::as_str) == Some(at)
        || p.get("btrfs").and_then(Value::as_array).into_iter().flatten().any(|s| mounted_at(s, at))
}

impl ManualDisk {
    /// Every partition the layout keeps or creates (not the ones it deletes).
    fn partitions(&self) -> Vec<&Value> {
        let mods = self.config.get("device_modifications").and_then(Value::as_array);
        mods.into_iter()
            .flatten()
            .flat_map(|d| d.get("partitions").and_then(Value::as_array).into_iter().flatten())
            .filter(|p| p.get("status").and_then(Value::as_str) != Some("delete"))
            .collect()
    }

    /// Why Bindle won't install with this layout: it touches the disk the
    /// ISO booted from, / is under 20 GiB, or (UEFI) there is no EFI
    /// partition. A pre-mounted layout is archinstall's to check.
    pub fn check(&self, medium: &Medium, uefi: bool) -> Result<(), String> {
        if self.config.get("mountpoint").is_some() {
            return Ok(());
        }
        let mods = self.config.get("device_modifications").and_then(Value::as_array);
        let devs = mods.into_iter().flatten().filter_map(|d| d.get("device").and_then(Value::as_str));
        if let Some(d) = devs.into_iter().find(|d| medium.contains(d)) {
            return Err(format!("{d} is the USB stick or disk this installer started from; leave it out"));
        }
        let parts = self.partitions();
        let lvm = self.config.pointer("/lvm_config/vol_groups").and_then(Value::as_array);
        let volumes =
            lvm.into_iter().flatten().flat_map(|g| g.get("volumes").and_then(Value::as_array).into_iter().flatten());
        let root = parts
            .iter()
            .find(|p| mounted_at(p, "/"))
            .and_then(|p| p.get("size"))
            .or_else(|| volumes.into_iter().find(|v| mounted_at(v, "/")).and_then(|v| v.get("length")));
        match root.map(bytes) {
            None => return Err("Nothing is mounted at /: give one partition the mount point /".into()),
            Some(Some(b)) if b < MIN_DISK_BYTES => {
                return Err(format!("/ is {:.1} GiB; Bindle needs 20 GiB at least", b as f64 / (1u64 << 30) as f64));
            }
            _ => {}
        }
        if uefi {
            let esp = parts.iter().any(|p| {
                let flags = p.get("flags").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str);
                p.get("fs_type").and_then(Value::as_str) == Some("fat32")
                    && (mounted_at(p, "/boot") || mounted_at(p, "/efi"))
                    && flags.into_iter().any(|f| f == "esp" || f == "boot")
            });
            if !esp {
                return Err("This computer starts with UEFI: the layout needs an EFI partition \
                     (FAT32, mounted at /boot or /efi, with the ESP/boot flag)"
                    .into());
            }
        }
        Ok(())
    }
}

/// Open archinstall's disk editor on the terminal (the wizard's screen must
/// be restored first). None: left without choosing a layout.
pub fn edit() -> Result<Option<ManualDisk>> {
    let dir = private_workdir()?;
    let (script, out) = (dir.join("bindle_disk_editor.py"), dir.join("disk.json"));
    let result = (|| {
        write_atomic(&script, EDITOR.as_bytes(), 0o600)?;
        let status = Command::new("python3")
            .arg(&script)
            .env("BINDLE_DISK_OUT", &out)
            .status()
            .context("start python3 for archinstall's disk editor")?;
        if !status.success() {
            bail!(
                "archinstall's disk editor couldn't open ({status}). For manual partitioning use `install.sh iso`, \
                 which opens archinstall's whole menu."
            );
        }
        match fs::read_to_string(&out) {
            Ok(text) => ManualDisk::parse(&text).map(Some).context("the disk editor saved a layout Bindle can't read"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("read the layout from the disk editor"),
        }
    })();
    // The saved layout can hold the disk passphrase: never left behind.
    let _ = fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAVED: &str = r#"{"disk_config":{"config_type":"manual_partitioning","device_modifications":[
        {"device":"/dev/nvme0n1","wipe":false,"partitions":[{"obj_id":"a"},{"obj_id":"b"},{"obj_id":"c"}]},
        {"device":"/dev/sda","wipe":true,"partitions":[{"obj_id":"d"}]}],
        "disk_encryption":{"encryption_type":"luks","partitions":["b"]}},"encryption_password":"hunter22"}"#;

    #[test]
    fn reads_what_the_editor_saved() {
        let m = ManualDisk::parse(SAVED).unwrap();
        assert_eq!(m.passphrase.as_deref(), Some("hunter22"));
        assert_eq!(m.config["config_type"], "manual_partitioning");
        assert_eq!(m.devices(), "/dev/nvme0n1, /dev/sda");
        let lines = m.lines();
        assert_eq!(lines[0], "/dev/nvme0n1: 3 partitions");
        assert_eq!(lines[1], "/dev/sda: 1 partition, erased first");
        assert!(lines[2].starts_with("Encrypted"));
    }

    #[test]
    fn premounted_and_broken_output() {
        let m =
            ManualDisk::parse(r#"{"disk_config":{"config_type":"pre_mounted_config","mountpoint":"/mnt"}}"#).unwrap();
        assert!(m.passphrase.is_none());
        assert!(m.lines()[0].contains("/mnt"));
        assert_eq!(m.devices(), "the mounted disks");
        assert!(ManualDisk::parse("{}").is_none());
        assert!(ManualDisk::parse("not json").is_none());
        assert!(ManualDisk::parse(r#"{"disk_config":{"device_modifications":[]}}"#).is_none());
    }

    fn layout(parts: &str, extra: &str) -> ManualDisk {
        let saved = format!(
            r#"{{"disk_config":{{"config_type":"manual_partitioning",{extra}"device_modifications":[
            {{"device":"/dev/nvme0n1","wipe":true,"partitions":[{parts}]}}]}}}}"#
        );
        ManualDisk::parse(&saved).unwrap()
    }

    const ESP: &str = r#"{"status":"create","fs_type":"fat32","mountpoint":"/boot","flags":["boot","esp"],
        "size":{"value":1024,"unit":"MiB","sector_size":{"value":512,"unit":"B"}}}"#;
    const ROOT: &str = r#"{"status":"create","fs_type":"btrfs","mountpoint":null,"flags":[],
        "btrfs":[{"name":"@","mountpoint":"/"}],"size":{"value":100,"unit":"GiB","sector_size":null}}"#;

    #[test]
    fn guards_a_manual_layout() {
        let ok = layout(&format!("{ESP},{ROOT}"), "");
        assert_eq!(ok.check(&Medium::NotLive, true), Ok(()));
        let usb = Medium::Disks(vec!["/dev/nvme0n1".into()]);
        assert!(ok.check(&usb, true).unwrap_err().contains("started from"));
        // No EFI partition: fine on BIOS, refused on UEFI.
        let no_esp = layout(ROOT, "");
        assert_eq!(no_esp.check(&Medium::NotLive, false), Ok(()));
        assert!(no_esp.check(&Medium::NotLive, true).unwrap_err().contains("EFI partition"));
        let ext4_esp = layout(&format!("{},{ROOT}", ESP.replace("fat32", "ext4")), "");
        assert!(ext4_esp.check(&Medium::NotLive, true).is_err());
        let efi = layout(&format!("{},{ROOT}", ESP.replace("\"/boot\"", "\"/efi\"")), "");
        assert_eq!(efi.check(&Medium::NotLive, true), Ok(()));
        // A small root, in sectors.
        let small = ROOT.replace(
            r#"{"value":100,"unit":"GiB","sector_size":null}"#,
            r#"{"value":20971520,"unit":"sectors","sector_size":{"value":512,"unit":"B"}}"#,
        );
        let err = layout(&format!("{ESP},{small}"), "").check(&Medium::NotLive, true).unwrap_err();
        assert!(err.contains("10.0 GiB") && err.contains("20 GiB"), "{err}");
        // A partition being deleted doesn't count; nothing at / is refused.
        let gone = ROOT.replace(r#""status":"create""#, r#""status":"delete""#);
        assert!(
            layout(&format!("{ESP},{gone}"), "").check(&Medium::NotLive, true).unwrap_err().contains("mount point /")
        );
        // Root on LVM: its volume's length counts.
        let lvm = r#""lvm_config":{"vol_groups":[{"name":"vg","volumes":[{"name":"root","mountpoint":"/",
            "length":{"value":8,"unit":"GiB","sector_size":null}}]}]},"#;
        assert!(layout(ESP, lvm).check(&Medium::NotLive, true).unwrap_err().contains("8.0 GiB"));
        let premounted =
            ManualDisk::parse(r#"{"disk_config":{"config_type":"pre_mounted_config","mountpoint":"/mnt"}}"#);
        assert_eq!(premounted.unwrap().check(&usb, true), Ok(()));
    }

    #[test]
    fn the_helper_is_embedded() {
        assert!(EDITOR.contains("DiskLayoutConfigurationMenu") && EDITOR.contains("BINDLE_DISK_OUT"));
    }
}
