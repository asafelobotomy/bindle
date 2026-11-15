//! `bindle rollback --installed-system`: from the live system the boot
//! menu's "Roll back the system" starts (`bindle.rollback=1`), roll back the
//! computer's own system, which isn't running. Already root here; the disk
//! is the one the image came from (`img_dev=`).

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use bindle_core::recover::rollback::{self, Layout};
use bindle_core::recover::snapper::{self, Snapshot};

const PEEK: &str = "/run/bindle/rollback-peek";

fn mount_top(device: &str) -> Result<()> {
    fs::create_dir_all(PEEK)?;
    let ok = Command::new("mount").args(["-o", "ro,subvolid=5", device, PEEK]).status()?.success();
    if !ok {
        bail!("couldn't mount {device}");
    }
    Ok(())
}

/// The system's layout (its fstab, in whichever subvolume holds one with a
/// Btrfs / entry) and its snapshots, read from the filesystem's top.
fn look(device: &str) -> Result<(Layout, Vec<Snapshot>)> {
    mount_top(device)?;
    let found = (|| {
        let mut candidates = vec![Path::new(PEEK).join("@/etc/fstab")];
        for e in fs::read_dir(PEEK)?.flatten() {
            candidates.push(e.path().join("etc/fstab"));
        }
        let layout = candidates
            .iter()
            .filter_map(|p| fs::read_to_string(p).ok())
            .find_map(|f| rollback::layout_from_fstab(&f, device).ok())
            .ok_or_else(|| {
                anyhow!("no system on {device} that Bindle can roll back (Btrfs, root in a subvolume like @)")
            })?;
        let mut snaps: Vec<Snapshot> = fs::read_dir(Path::new(PEEK).join(&layout.snapshots))
            .with_context(|| format!("read {}", layout.snapshots))?
            .flatten()
            .filter_map(|e| fs::read_to_string(e.path().join("info.xml")).ok())
            .filter_map(|x| snapper::from_info(&x))
            .filter(|s| s.number != 0)
            .collect();
        snaps.sort_by_key(|s| std::cmp::Reverse(s.number));
        Ok((layout, snaps))
    })();
    let _ = Command::new("umount").arg(PEEK).status();
    found
}

pub fn run(number: Option<u32>, yes: bool, dry_run: bool) -> Result<()> {
    let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let device = cmdline
        .split_whitespace()
        .find_map(|w| w.strip_prefix("img_dev="))
        .ok_or_else(|| anyhow!("not started from Bindle's boot menu entry (no img_dev= on the command line)"))?
        .to_string();
    let (layout, snaps) = look(&device)?;
    println!("Rolling back this computer's system ({device}).");
    super::show(&snaps);
    let number = match number {
        Some(n) => n,
        None if snaps.is_empty() => bail!("there's no snapshot to roll back to"),
        None => super::pick(&snaps)?,
    };
    if !snaps.iter().any(|s| s.number == number) {
        bail!("there's no snapshot {number}");
    }
    if !yes && !crate::apply::confirm(&format!("Roll back to snapshot {number}?"), false) {
        bail!("nothing was changed");
    }
    let stamp = bindle_run::sys::now_id();
    let script = rollback::script(&layout, number, &stamp);
    if dry_run {
        println!("would run as root:\n{script}");
        return Ok(());
    }
    if !Command::new("sh").arg("-ec").arg(&script).status()?.success() {
        bail!("the rollback didn't finish; the system is as it was");
    }
    Ok(())
}
