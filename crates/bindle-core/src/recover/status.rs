//! What this computer allows for recovery, worked out from the mount table:
//! Roll back needs a Btrfs root in a subvolume and snapper's root config;
//! Factory Restore an unencrypted root. The Recovery window and uQ
//! Settings' Restore page (`bindle recovery --status`) both read this.

use std::path::Path;

use serde_json::{Value, json};

use super::restore::IMAGE;
use super::rollback;

/// snapper's root config: there are snapshots to list, take and roll back to.
pub const SNAPPER_CONFIG: &str = "/etc/snapper/configs/root";

/// `{rollback, restore, imageKept, snapper}`: `rollback` and `restore` are
/// null when allowed, else why not; `snapper` is whether snapshots work here.
pub fn status() -> Value {
    let info = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    from_parts(&info, Path::new(SNAPPER_CONFIG).exists(), Path::new(IMAGE).is_file())
}

/// [`status`] from a mount table and what's on the disk.
pub fn from_parts(mountinfo: &str, snapper: bool, image_kept: bool) -> Value {
    let layout = rollback::layout(mountinfo);
    let rollback = match &layout {
        Err(why) => Some(why.clone()),
        Ok(_) if !snapper => Some(
            "snapper isn't set up here, so there are no snapshots (Bindle's \"Btrfs snapshots\" module sets it up)"
                .to_string(),
        ),
        Ok(_) => None,
    };
    let root_dev =
        mountinfo.lines().find(|l| l.split_whitespace().nth(4) == Some("/")).and_then(|l| l.split(" - ").nth(1));
    let encrypted = root_dev.and_then(|r| r.split_whitespace().nth(1)).is_some_and(|d| d.starts_with("/dev/mapper/"));
    json!({
        "rollback": rollback,
        "restore": encrypted.then_some("this computer's disk is encrypted: start the installer from a USB stick instead (see bindle iso)"),
        "imageKept": image_kept,
        "snapper": snapper,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXT4: &str = "22 1 259:2 / / rw,relatime shared:1 - ext4 /dev/nvme0n1p2 rw\n";
    const LUKS: &str = "22 1 254:0 / / rw,relatime shared:1 - ext4 /dev/mapper/root rw\n";

    #[test]
    fn no_btrfs_no_rollback() {
        let s = from_parts(EXT4, true, false);
        assert!(s["rollback"].is_string());
        assert!(s["restore"].is_null());
        assert_eq!(s["snapper"], true);
        assert_eq!(s["imageKept"], false);
    }

    #[test]
    fn encrypted_root_refuses_restore() {
        assert!(from_parts(LUKS, false, true)["restore"].is_string());
        assert_eq!(from_parts(LUKS, false, true)["imageKept"], true);
    }
}
