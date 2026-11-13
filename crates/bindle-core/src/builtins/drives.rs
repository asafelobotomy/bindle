//! Drives to mount at startup: one generated module per detected filesystem
//! (`mount-<uuid>`), each expanding to its `/etc/fstab` line.
//!
//! Every line mounts by `UUID=` with `nofail`, so a drive that is missing or
//! broken never stops the boot (systemd neither requires the mount nor waits
//! for it). The rest depends on the filesystem; see `mount_options`.

use std::collections::{BTreeMap, BTreeSet};

use super::Expansion;
use crate::catalog::Module;
use crate::detect::{Drive, Facts};
use crate::fstab::Entry;

/// Module ids are `mount-<Drive::key>`; the builtin is `mount:<key>`.
pub const ID_PREFIX: &str = "mount-";
pub const BUILTIN_PREFIX: &str = "mount:";
/// Category and group the modules appear under.
pub const CATEGORY: &str = "hardware";
pub const GROUP: &str = "Drives to mount at startup";
/// Where drives are mounted: `/mnt/<label>`.
pub const MOUNT_ROOT: &str = "/mnt";
/// How long systemd waits for the device before giving the mount up (the
/// default is 90 s). Nothing waits on a `nofail` mount, so this only bounds a
/// job in the background; it is long enough for a disk to spin up.
const DEVICE_TIMEOUT: &str = "x-systemd.device-timeout=30s";

/// (fstab type, options before the common ones, fsck pass) per filesystem.
fn mount_options<'a>(d: &'a Drive, f: &Facts) -> (&'a str, String, u8) {
    // No owners or modes on disk: everything belongs to the user. Files stay
    // executable (umask, not fmask), or games and scripts kept there won't run.
    let owned = format!("uid={},gid={},umask=022", f.uid, f.gid);
    match d.fstype.as_str() {
        // fsck after the root filesystem; the others check themselves on mount.
        fs @ ("ext4" | "ext3" | "ext2") => (fs, "defaults".into(), 2),
        // Compress what is written from now on; noatime keeps reads from
        // rewriting metadata that snapshots share.
        "btrfs" => ("btrfs", "defaults,noatime,compress=zstd".into(), 0),
        // The kernel's own driver (ntfs3, since 5.15): no ntfs-3g needed.
        // windows_names refuses file names Windows can't open.
        "ntfs" => ("ntfs3", format!("{owned},windows_names"), 0),
        "exfat" => ("exfat", owned, 0),
        "vfat" => ("vfat", format!("{owned},utf8"), 0),
        fs => (fs, "defaults".into(), 0),
    }
}

/// The drive's fstab line; `None` when Bindle doesn't offer to mount it.
pub fn entry(d: &Drive, f: &Facts) -> Option<Entry> {
    if d.blocked.is_some() {
        return None;
    }
    let (fstype, mut options, pass) = mount_options(d, f);
    options.push_str(",nofail,");
    options.push_str(DEVICE_TIMEOUT);
    if d.removable {
        // Mounted when first opened, not at boot, and let go when idle so it
        // can be unplugged: startup never waits for a drive that isn't there.
        options.push_str(",noauto,x-systemd.automount,x-systemd.idle-timeout=5min");
    }
    Some(Entry {
        uuid: d.uuid.clone(),
        dir: dirs(f).remove(&d.key())?,
        fstype: fstype.into(),
        options,
        pass,
        mount_now: d.mounted_at.is_empty() && !d.removable,
    })
}

/// A label as a directory name: no spaces or anything fstab would need escaped.
fn dir_name(d: &Drive) -> String {
    let name: String =
        d.label.trim().chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    if name.trim_matches(['_', '.']).is_empty() {
        let id: String = d.uuid.chars().filter(char::is_ascii_alphanumeric).take(8).collect();
        format!("drive-{}", id.to_lowercase())
    } else {
        name
    }
}

/// Mount point of every drive Bindle offers, by key. A drive already in
/// Bindle's fstab block keeps the one it has; the others get `/mnt/<label>`,
/// with a number added when fstab or another drive already uses that.
fn dirs(f: &Facts) -> BTreeMap<String, String> {
    let mut taken: BTreeSet<String> = f.fstab_dirs.iter().cloned().collect();
    let mut out = BTreeMap::new();
    let offered = || f.drives.iter().filter(|d| d.blocked.is_none());
    for d in offered() {
        if let Some(dir) = d.fstab_dir.as_ref().filter(|_| d.managed) {
            out.insert(d.key(), dir.clone());
        }
    }
    for d in offered().filter(|d| !d.managed) {
        let base = format!("{MOUNT_ROOT}/{}", dir_name(d));
        let dir = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") }).find(|c| !taken.contains(c));
        let dir = dir.expect("an unbounded range");
        taken.insert(dir.clone());
        out.insert(d.key(), dir);
    }
    out
}

pub(super) fn expand(key: &str, f: &Facts) -> Expansion {
    let mounts = f.drives.iter().find(|d| d.key() == key).and_then(|d| entry(d, f)).into_iter().collect();
    Expansion { mounts, ..Default::default() }
}

/// `931 GiB`, `64 GiB`, `512 MiB`.
pub fn size_label(bytes: u64) -> String {
    const GIB: f64 = (1u64 << 30) as f64;
    let gib = bytes as f64 / GIB;
    if gib >= 1024.0 {
        format!("{:.1} TiB", gib / 1024.0)
    } else if gib >= 1.0 {
        format!("{gib:.0} GiB")
    } else {
        format!("{:.0} MiB", gib * 1024.0)
    }
}

fn fs_name(fstype: &str) -> &str {
    match fstype {
        "btrfs" => "Btrfs",
        "ntfs" => "NTFS",
        "exfat" => "exFAT",
        "vfat" => "FAT",
        "xfs" => "XFS",
        "f2fs" => "F2FS",
        "crypto_LUKS" => "LUKS-encrypted",
        other => other,
    }
}

/// One module per detected drive, so each can be ticked like any other.
pub fn modules(f: &Facts) -> Vec<Module> {
    let dirs = dirs(f);
    f.drives
        .iter()
        .map(|d| {
            let key = d.key();
            let label = if d.label.trim().is_empty() { "unnamed drive" } else { d.label.trim() };
            let kind = match (d.removable, d.rotational) {
                (true, _) => "a removable drive",
                (false, true) => "a hard disk",
                (false, false) => "an SSD",
            };
            let on = if d.model.is_empty() { kind.to_string() } else { format!("{kind} ({})", d.model) };
            let what = format!("{} on {on}.", d.device);
            let mut notes = Vec::new();
            let description = match dirs.get(&key) {
                Some(dir) if d.removable => {
                    notes.push(
                        "Removable: mounted when first opened rather than at startup, and released after 5 idle minutes. \
                         The file manager stops mounting it on its own."
                            .into(),
                    );
                    format!("{what} Opens at {dir} whenever it is plugged in.")
                }
                Some(dir) => format!("{what} Mounted at {dir} every time the computer starts."),
                None => what,
            };
            if d.blocked.is_none() {
                match d.fstype.as_str() {
                    "ntfs" => notes.push(
                        "Windows drive: turn off Fast Startup in Windows (and don't hibernate it), \
                         or it leaves the drive locked and Linux can't mount it."
                            .into(),
                    ),
                    "btrfs" => notes
                        .push("Mounts the filesystem's default subvolume; new files are compressed (zstd).".into()),
                    _ => {}
                }
                if let (Some(now), Some(dir)) = (d.mounted_at.first(), dirs.get(&key))
                    && now != dir
                {
                    notes.push(format!("Mounted at {now} right now; it is at {dir} from the next start."));
                }
            }
            Module {
                id: format!("{ID_PREFIX}{key}"),
                category: CATEGORY.into(),
                group: GROUP.into(),
                name: format!("Mount {label} ({} {})", size_label(d.size), fs_name(&d.fstype)),
                description,
                when: vec![format!("mountable:{key}")],
                builtin: Some(format!("{BUILTIN_PREFIX}{key}")),
                notes,
                ..Default::default()
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "drives_tests.rs"]
mod tests;
