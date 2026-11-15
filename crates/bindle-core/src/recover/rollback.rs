//! Rolling the system back to a snapper snapshot, on a Btrfs root kept in a
//! subvolume of its own (archinstall's `@`). The running root subvolume is
//! renamed aside (never deleted: `bindo rollback-cleanup` does that once the
//! user is happy) and a writable snapshot of the chosen one takes its name,
//! so the next boot mounts it. When /boot is a partition of its own (the
//! ESP, Bindle's default), the kernels there belong to no snapshot: the
//! rolled-back system's own kernels and boot images are put there, or it
//! would boot a kernel whose modules it doesn't have. A failure part way
//! puts everything back as it was.

/// Where things are, read from /proc/self/mountinfo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The block device the Btrfs filesystem is on (/dev/mapper/… when encrypted).
    pub device: String,
    /// The root subvolume's path from the top level, e.g. `@`.
    pub root: String,
    /// The directory holding snapper's `<N>/snapshot`, from the top level:
    /// `@.snapshots` (its own subvolume) or `@/.snapshots` (inside root).
    pub snapshots: String,
    /// Where /boot is: its kernels belong to no snapshot when it is a
    /// partition of its own.
    pub boot: Boot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Boot {
    /// Inside the root subvolume: rolled back with it.
    InRoot,
    /// Its own partition, mounted on /boot (the running system).
    Mounted,
    /// Its own partition, this device (from the live system: not mounted).
    Device(String),
}

struct Mount<'a> {
    root: &'a str,
    point: &'a str,
    fstype: &'a str,
    source: &'a str,
}

fn mounts(mountinfo: &str) -> impl Iterator<Item = Mount<'_>> {
    mountinfo.lines().filter_map(|l| {
        let (left, right) = l.split_once(" - ")?;
        let left: Vec<&str> = left.split_whitespace().collect();
        let mut right = right.split_whitespace();
        Some(Mount { root: left.get(3)?, point: left.get(4)?, fstype: right.next()?, source: right.next()? })
    })
}

/// The layout, or why this system can't be rolled back by Bindle.
pub fn layout(mountinfo: &str) -> Result<Layout, String> {
    // The last mount of a point is the one in effect.
    let at = |point: &str| mounts(mountinfo).filter(|m| m.point == point).last();
    let root = at("/").ok_or("/ isn't in the mount table")?;
    if root.fstype != "btrfs" {
        return Err(format!("the system is on {}, not Btrfs: there are no snapshots to roll back to", root.fstype));
    }
    let sub = root.root.trim_start_matches('/');
    if sub.is_empty() || sub.contains('/') {
        return Err(format!(
            "the system runs from subvolume /{sub}, not one at the top of the filesystem (like @): roll back with snapper's own tools"
        ));
    }
    let snapshots = match at("/.snapshots") {
        Some(m) if m.fstype == "btrfs" && m.source == root.source => m.root.trim_start_matches('/').to_string(),
        Some(_) => return Err("/.snapshots isn't on the system's own Btrfs filesystem".into()),
        None => format!("{sub}/.snapshots"),
    };
    let boot = if at("/boot").is_some_and(|m| m.source != root.source) { Boot::Mounted } else { Boot::InRoot };
    Ok(Layout { device: root.source.to_string(), root: sub.to_string(), snapshots, boot })
}

/// The layout of a system that isn't running (from the live system: Rollback
/// from the boot menu), from its fstab. `device`: its Btrfs filesystem.
pub fn layout_from_fstab(fstab: &str, device: &str) -> Result<Layout, String> {
    let entries: Vec<Vec<&str>> = fstab
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .filter(|f| f.len() >= 4 && !f[0].starts_with('#'))
        .collect();
    let at = |point: &str| entries.iter().find(|f| f[1] == point);
    let subvol = |f: &Vec<&str>| {
        f[3].split(',').find_map(|o| o.strip_prefix("subvol=")).map(|s| s.trim_start_matches('/').to_string())
    };
    let root = at("/").ok_or("its fstab has no / entry")?;
    if root[2] != "btrfs" {
        return Err(format!("the system is on {}, not Btrfs: there are no snapshots to roll back to", root[2]));
    }
    let sub =
        subvol(root).filter(|s| !s.is_empty() && !s.contains('/')).ok_or("its root isn't a Btrfs subvolume like @")?;
    let snapshots = match at("/.snapshots") {
        Some(f) if f[2] == "btrfs" => subvol(f).ok_or("its /.snapshots entry names no subvolume")?,
        _ => format!("{sub}/.snapshots"),
    };
    let boot = match at("/boot") {
        Some(f) if f[2] != "btrfs" => {
            let dev = match f[0].split_once('=') {
                Some(("UUID", u)) => format!("/dev/disk/by-uuid/{u}"),
                Some(("PARTUUID", u)) => format!("/dev/disk/by-partuuid/{u}"),
                Some(("LABEL", l)) => format!("/dev/disk/by-label/{l}"),
                _ => f[0].to_string(),
            };
            Boot::Device(dev)
        }
        _ => Boot::InRoot,
    };
    Ok(Layout { device: device.to_string(), root: sub, snapshots, boot })
}

/// Shell-quote one word.
fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The name the running root is moved to.
pub fn aside(layout: &Layout, stamp: &str) -> String {
    format!("{}.bindle-before-rollback-{stamp}", layout.root)
}

/// The script (for `sh -ec`) that makes snapshot `number` the root at the
/// next boot. `stamp` names the old root's new place (`aside`).
pub fn script(layout: &Layout, number: u32, stamp: &str) -> String {
    let (root, old) = (&layout.root, aside(layout, stamp));
    // Inside the root subvolume, snapper's directory moves with it.
    let nested = layout.snapshots.starts_with(&format!("{root}/"));
    let snaps = if nested { format!("{old}{}", &layout.snapshots[root.len()..]) } else { layout.snapshots.clone() };
    let mut s = format!(
        r#"dev={dev}; root={root}; old={old}; snap={snap}
top=/run/bindle/rollback-top; new=/run/bindle/rollback-root; keep=/run/bindle/rollback-boot
bootdir=/boot
swapped=0
undo() {{
    status=$?
    if [ "$swapped" = 1 ] && [ "$status" != 0 ]; then
        echo "Rolling back failed; putting the system back as it was."
        btrfs subvolume delete "$top/$root" >/dev/null 2>&1 || true
        mv "$top/$old" "$top/$root" || true
        if [ -d "$keep" ]; then cp -a "$keep"/. "$bootdir"/ || true; fi
    fi
    umount -R "$new" 2>/dev/null || true
    [ "$bootdir" = /boot ] || umount "$bootdir" 2>/dev/null || true
    umount "$top" 2>/dev/null || true
    rm -rf "$keep"
    exit "$status"
}}
trap undo EXIT
mkdir -p "$top" "$new"
mount -o subvolid=5 "$dev" "$top"
btrfs subvolume show "$top"/{now_snap}/{number}/snapshot >/dev/null
[ ! -e "$top/$old" ]
mv "$top/$root" "$top/$old"
swapped=1
btrfs subvolume snapshot "$top/$snap/{number}/snapshot" "$top/$root"
"#,
        dev = q(&layout.device),
        root = q(root),
        old = q(&old),
        snap = q(&snaps),
        now_snap = q(&layout.snapshots),
    );
    if nested {
        // The new root holds an empty stand-in for the .snapshots subvolume:
        // the snapshots themselves move into it.
        s.push_str("rmdir \"$top/$root/.snapshots\" 2>/dev/null || true\nmv \"$top/$old/.snapshots\" \"$top/$root/.snapshots\"\n");
    }
    // fstab may pin the root by subvolume id, which the new root doesn't have.
    s.push_str(
        r#"f="$top/$root/etc/fstab"
if [ -f "$f" ]; then
    awk 'BEGIN { OFS = "\t" } !/^[[:space:]]*#/ && $2 == "/" && $3 == "btrfs" { gsub(/(^|,)subvolid=[0-9]+/, "", $4); sub(/^,/, "", $4) } { print }' "$f" >"$f.bindle" && mv "$f.bindle" "$f"
fi
"#,
    );
    if let Boot::Device(dev) = &layout.boot {
        s.push_str(&format!(
            "bootdir=/run/bindle/rollback-esp\nmkdir -p \"$bootdir\"\nmount {} \"$bootdir\"\n",
            q(dev)
        ));
    }
    if layout.boot != Boot::InRoot {
        s.push_str(
            r#"# /boot holds no snapshot: the rolled-back system's own kernels go there.
mkdir -p "$keep"
cp -a "$bootdir"/. "$keep"/
mount -o "subvol=/$root" "$dev" "$new"
mount --bind "$bootdir" "$new/boot"
for d in proc sys dev run; do mount --rbind "/$d" "$new/$d"; mount --make-rslave "$new/$d"; done
for m in "$new"/usr/lib/modules/*/; do
    [ -f "$m/pkgbase" ] && [ -f "$m/vmlinuz" ] || continue
    install -Dm644 "$m/vmlinuz" "$new/boot/vmlinuz-$(cat "$m/pkgbase")"
done
chroot "$new" mkinitcpio -P
"#,
        );
    }
    s.push_str(&format!(
        "echo \"Snapshot {number} is the system from the next start. The one before is kept as $old.\"\n"
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCHINSTALL: &str = "\
33 2 0:30 /@ / rw,relatime shared:1 - btrfs /dev/vda2 rw,compress=zstd:3,subvolid=256,subvol=/@
40 33 0:30 /@.snapshots /.snapshots rw,relatime shared:20 - btrfs /dev/vda2 rw,subvolid=260,subvol=/@.snapshots
41 33 259:1 / /boot rw,relatime shared:21 - vfat /dev/vda1 rw,fmask=0022
";

    #[test]
    fn archinstalls_layout() {
        let l = layout(ARCHINSTALL).unwrap();
        assert_eq!(
            l,
            Layout {
                device: "/dev/vda2".into(),
                root: "@".into(),
                snapshots: "@.snapshots".into(),
                boot: Boot::Mounted
            }
        );
        let s = script(&l, 12, "20261115-1000");
        assert!(s.contains("mv \"$top/$root\" \"$top/$old\""));
        assert!(s.contains("old='@.bindle-before-rollback-20261115-1000'"));
        assert!(s.contains("\"$top/$snap/12/snapshot\" \"$top/$root\""));
        assert!(s.contains("chroot \"$new\" mkinitcpio -P"), "the ESP's kernels follow the snapshot");
        assert!(!s.contains("mv \"$top/$old/.snapshots\""));
    }

    #[test]
    fn snapshots_inside_the_root_move_with_it() {
        let info = "33 2 0:30 /@ / rw - btrfs /dev/mapper/root rw\n";
        let l = layout(info).unwrap();
        assert_eq!((l.snapshots.as_str(), &l.boot), ("@/.snapshots", &Boot::InRoot));
        let s = script(&l, 3, "x");
        assert!(s.contains("snap='@.bindle-before-rollback-x/.snapshots'"), "{s}");
        assert!(s.contains("btrfs subvolume show \"$top\"/'@/.snapshots'/3/snapshot"), "{s}");
        assert!(s.contains("mv \"$top/$old/.snapshots\" \"$top/$root/.snapshots\""));
        assert!(!s.contains("mkinitcpio"), "/boot is part of the snapshot");
    }

    #[test]
    fn from_the_live_system_by_fstab() {
        let fstab = "# /dev/vda2\nUUID=r /  btrfs rw,relatime,compress=zstd:3,subvolid=256,subvol=/@ 0 0\n\
                     UUID=r /.snapshots btrfs rw,subvol=/@.snapshots 0 0\nUUID=AB-CD /boot vfat rw,fmask=0022 0 2\n";
        let l = layout_from_fstab(fstab, "/dev/disk/by-uuid/r").unwrap();
        assert_eq!(l.root, "@");
        assert_eq!(l.snapshots, "@.snapshots");
        assert_eq!(l.boot, Boot::Device("/dev/disk/by-uuid/AB-CD".into()));
        let s = script(&l, 2, "x");
        assert!(s.contains("mount '/dev/disk/by-uuid/AB-CD' \"$bootdir\""), "{s}");
        assert!(s.contains("mount --bind \"$bootdir\" \"$new/boot\""));
        assert!(layout_from_fstab("UUID=r / ext4 rw 0 1\n", "x").unwrap_err().contains("ext4"));
        assert!(layout_from_fstab("UUID=r / btrfs rw 0 0\n", "x").is_err(), "no subvolume");
    }

    #[test]
    fn what_cant_be_rolled_back() {
        assert!(layout("33 2 8:2 / / rw - ext4 /dev/sda2 rw\n").unwrap_err().contains("ext4"));
        assert!(layout("33 2 0:30 / / rw - btrfs /dev/sda2 rw\n").is_err(), "the top level itself");
        let snapper_layout = "33 2 0:30 /@/.snapshots/5/snapshot / rw - btrfs /dev/sda2 rw\n";
        assert!(layout(snapper_layout).unwrap_err().contains("snapper's own tools"));
        assert!(layout("").is_err());
    }

    /// Every variant parses as shell (`sh -n`); BINDLE_SHOW_SCRIPT=dir writes them there.
    #[test]
    fn scripts_are_valid_shell() {
        let nested = layout("33 2 0:30 /@ / rw - btrfs /dev/vda2 rw\n").unwrap();
        let live = Layout { boot: Boot::Device("/dev/vda1".into()), ..layout(ARCHINSTALL).unwrap() };
        for (i, l) in [layout(ARCHINSTALL).unwrap(), nested, live].iter().enumerate() {
            let text = script(l, 4, "s");
            if let Some(dir) = std::env::var_os("BINDLE_SHOW_SCRIPT") {
                std::fs::write(std::path::Path::new(&dir).join(format!("rollback-{i}.sh")), &text).unwrap();
            }
            let ok = std::process::Command::new("sh").args(["-n", "-c", &text]).status().unwrap().success();
            assert!(ok, "{text}");
        }
    }

    #[test]
    fn quotes_hold() {
        assert_eq!(q("a'b"), r"'a'\''b'");
    }
}
