//! "Manual partitioning" on the Disk page: archinstall's own disk editor for
//! that one step. A small Python helper opens the editor from the ISO's
//! archinstall and saves the layout in archinstall's own format; nothing
//! touches a disk until Install is confirmed on Review.

use std::fs;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::apply::sys::write_atomic;
use crate::iso::private_workdir;

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

    #[test]
    fn the_helper_is_embedded() {
        assert!(EDITOR.contains("DiskLayoutConfigurationMenu") && EDITOR.contains("BINDLE_DISK_OUT"));
    }
}
