//! Filesystems on drives and partitions the system doesn't use itself: what
//! Bindle can offer to mount at startup. Read from `lsblk` (the udev
//! database, no root) and `/etc/fstab`.

use std::collections::BTreeSet;
use std::process::Command;

use serde::Serialize;
use serde_json::Value;

use crate::fstab;

/// Filesystems Bindle writes an fstab line for.
pub const MOUNTABLE: &[&str] = &["ext4", "ext3", "ext2", "xfs", "btrfs", "f2fs", "ntfs", "exfat", "vfat"];

/// Partition types that are never a data drive: EFI system, Microsoft
/// reserved, Windows recovery, BIOS boot, Linux swap (GPT GUIDs, then MBR ids).
const RESERVED_PARTTYPES: &[&str] = &[
    "c12a7328-f81f-11d2-ba4b-00a0c93ec93b",
    "e3c9e316-0b5c-4db8-817d-f92df00215ae",
    "de94bba4-06d1-4d40-a16a-bfd50179d6ac",
    "21686148-6449-6e6f-744e-656564454649",
    "0657fd6d-a4ab-43c4-84e5-0933c84b4f4f",
    "0xef",
    "0x27",
    "0x82",
];

/// Where desktops and people mount extra drives; a filesystem mounted
/// anywhere else is one the running system uses.
const USER_MOUNTS: &[&str] = &["/mnt", "/media", "/run/media"];

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Drive {
    /// `/dev/sdb1`.
    pub device: String,
    /// Filesystem UUID; empty for a volume that has none (BitLocker).
    pub uuid: String,
    /// As blkid names it: `ext4`, `ntfs`, `crypto_LUKS`…
    pub fstype: String,
    pub label: String,
    /// Bytes.
    pub size: u64,
    /// USB, hot-pluggable or removable media.
    pub removable: bool,
    /// A spinning disk (false: SSD or flash).
    pub rotational: bool,
    /// Model of the disk it is on.
    pub model: String,
    /// Where it is mounted right now.
    pub mounted_at: Vec<String>,
    /// Where an `/etc/fstab` line already mounts it.
    pub fstab_dir: Option<String>,
    /// That line is in Bindle's own block.
    pub managed: bool,
    /// Why Bindle doesn't offer to mount it (encrypted, already in fstab).
    pub blocked: Option<String>,
}

impl Drive {
    /// What identifies the drive in a module id: the UUID, else the device name.
    pub fn key(&self) -> String {
        if self.uuid.is_empty() {
            self.device.trim_start_matches("/dev/").replace('/', "-")
        } else {
            self.uuid.to_lowercase()
        }
    }
}

pub(super) fn detect(fstab_text: &str) -> Vec<Drive> {
    const COLUMNS: &str = "PATH,TYPE,FSTYPE,UUID,PARTUUID,LABEL,SIZE,RM,ROTA,HOTPLUG,TRAN,MODEL,MOUNTPOINTS,PARTTYPE";
    Command::new("lsblk")
        .args(["--json", "--bytes", "--output", COLUMNS])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| drives_from(&String::from_utf8_lossy(&o.stdout), fstab_text))
        .unwrap_or_default()
}

fn text<'a>(node: &'a Value, key: &str) -> &'a str {
    node[key].as_str().unwrap_or_default()
}

/// lsblk prints booleans as `true` or, before util-linux 2.37, as `"1"`.
fn flag(node: &Value, key: &str) -> bool {
    node[key].as_bool().unwrap_or_else(|| text(node, key) == "1")
}

fn mountpoints(node: &Value) -> Vec<String> {
    node["mountpoints"].as_array().into_iter().flatten().filter_map(|m| m.as_str()).map(String::from).collect()
}

fn user_mount(dir: &str) -> bool {
    USER_MOUNTS.iter().any(|base| dir.strip_prefix(base).is_some_and(|rest| rest.starts_with('/')))
}

/// What the whole-disk node at the top of the tree says about its partitions.
#[derive(Clone, Default)]
struct Disk {
    removable: bool,
    rotational: bool,
    model: String,
    /// Below an encrypted container.
    encrypted: bool,
}

struct Found {
    drive: Drive,
    parttype: String,
    partuuid: String,
    /// It, or something inside it, is mounted where the system uses it (`/`, `/boot`, swap).
    system: bool,
}

/// Collects every filesystem in the subtree; true when the system uses any of it.
fn walk(node: &Value, disk: Option<&Disk>, out: &mut Vec<Found>) -> bool {
    let kind = text(node, "type");
    if matches!(kind, "loop" | "rom") {
        return false;
    }
    let mut disk = disk.cloned().unwrap_or_else(|| Disk {
        removable: flag(node, "rm") || flag(node, "hotplug") || text(node, "tran") == "usb",
        rotational: flag(node, "rota"),
        model: text(node, "model").trim().to_string(),
        encrypted: false,
    });
    let fstype = text(node, "fstype");
    let mounted_at = mountpoints(node);
    let mut system = mounted_at.iter().any(|m| !user_mount(m));
    // What is inside an encrypted container is reached through the container's entry.
    let own = (!fstype.is_empty() && !disk.encrypted).then(|| {
        out.push(Found {
            system,
            parttype: text(node, "parttype").to_lowercase(),
            partuuid: text(node, "partuuid").to_lowercase(),
            drive: Drive {
                device: text(node, "path").into(),
                uuid: text(node, "uuid").into(),
                fstype: fstype.into(),
                label: text(node, "label").into(),
                size: node["size"].as_u64().or_else(|| text(node, "size").parse().ok()).unwrap_or(0),
                removable: disk.removable,
                rotational: disk.rotational,
                model: disk.model.clone(),
                mounted_at,
                ..Default::default()
            },
        });
        out.len() - 1
    });
    disk.encrypted |= matches!(fstype, "crypto_LUKS" | "BitLocker") || kind == "crypt";
    for child in node["children"].as_array().into_iter().flatten() {
        system |= walk(child, Some(&disk), out);
    }
    // An unlocked container holding the root filesystem is the system's too.
    if let Some(i) = own {
        out[i].system = system;
    }
    system
}

fn encrypted(fstype: &str) -> Option<&'static str> {
    match fstype {
        "crypto_LUKS" => Some("LUKS"),
        "BitLocker" => Some("BitLocker"),
        _ => None,
    }
}

/// `lsblk --json --bytes` output and fstab's text → the drives to offer, in
/// lsblk's order. Left out: what the system itself uses, firmware and
/// recovery partitions, swap, and anything that isn't a mountable filesystem.
pub fn drives_from(lsblk: &str, fstab_text: &str) -> Vec<Drive> {
    let Ok(tree) = serde_json::from_str::<Value>(lsblk) else { return Vec::new() };
    let mut found = Vec::new();
    for node in tree["blockdevices"].as_array().into_iter().flatten() {
        walk(node, None, &mut found);
    }
    // A multi-device btrfs shows one UUID on every member: if one member is
    // the system's, all are; otherwise the first stands for the filesystem.
    let system: BTreeSet<String> =
        found.iter().filter(|f| f.system && !f.drive.uuid.is_empty()).map(|f| f.drive.uuid.clone()).collect();
    let lines = fstab::lines(fstab_text);
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for Found { mut drive, parttype, partuuid, system: in_use } in found {
        let offered = MOUNTABLE.contains(&drive.fstype.as_str()) && !drive.uuid.is_empty();
        if in_use
            || system.contains(&drive.uuid)
            || RESERVED_PARTTYPES.contains(&parttype.as_str())
            || !(offered || encrypted(&drive.fstype).is_some())
            || !seen.insert(drive.key())
        {
            continue;
        }
        let line = lines.iter().find(|l| {
            fstab::names_uuid(l.spec, &drive.uuid) && !drive.uuid.is_empty()
                || l.spec == drive.device
                || !drive.label.is_empty() && l.spec.strip_prefix("LABEL=") == Some(&drive.label)
                || !partuuid.is_empty()
                    && l.spec.strip_prefix("PARTUUID=").is_some_and(|p| p.eq_ignore_ascii_case(&partuuid))
        });
        drive.fstab_dir = line.map(|l| l.dir.to_string());
        drive.managed = line.is_some_and(|l| l.managed);
        drive.blocked = match (encrypted(&drive.fstype), line) {
            (Some(how), _) => Some(format!(
                "encrypted with {how}: unlocking it at startup needs a key and /etc/crypttab, which Bindle doesn't set up"
            )),
            (None, Some(l)) if !l.managed => Some(format!("already in /etc/fstab, mounted at {}", l.dir)),
            _ => None,
        };
        out.push(drive);
    }
    out
}

#[cfg(test)]
#[path = "drives_tests.rs"]
mod tests;
