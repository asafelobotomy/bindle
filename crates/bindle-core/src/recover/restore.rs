//! Factory Restore: start the Arch ISO from a file on this computer's own
//! disk, copied whole into memory (`copytoram`) so the disk is free to be
//! erased, and open `bindle install` in it. The ISO's kernel, a copy of
//! Bindle and a start script go in a folder on /boot (the boot loader reads
//! the kernel there); systemd mounts that folder in the live system
//! (`systemd.mount-extra`) and archiso runs the start script (`script=`).
//! The boot entry is used once: a restart without installing comes back to
//! the system as it was.

/// The ISO, on the root filesystem.
pub const IMAGE: &str = "/var/lib/bindle/restore/archlinux-x86_64.iso";
/// Kernel, initramfs, Bindle and the start script, on /boot.
pub const STAGE: &str = "/boot/bindle-restore";
/// Where the live system mounts the filesystem holding STAGE.
pub const LIVE_MOUNT: &str = "/run/bindle-restore";
/// systemd-boot's entry file (loader/entries/) and GRUB's menu entry id.
pub fn entry_id(mode: Mode) -> &'static str {
    match mode {
        Mode::Restore => "bindle-restore",
        Mode::Rollback => "bindle-rollback",
    }
}

/// What the live system is started for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `bindle install`, offering this computer's disk.
    Restore,
    /// `bindle rollback` on this computer's system (when it won't start).
    Rollback,
}

/// Where things are on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Where {
    /// The root filesystem's UUID and type.
    pub root_uuid: String,
    pub root_fstype: String,
    /// The root's Btrfs subvolume (e.g. `@`), when there is one.
    pub root_subvol: Option<String>,
    /// The filesystem /boot is on: its own partition (the ESP), or root.
    pub boot_uuid: String,
    pub boot_fstype: String,
    pub boot_apart: bool,
}

/// A path on the root filesystem as archiso's `img_loop` (relative to the
/// filesystem's top, where a Btrfs subvolume is a directory).
pub fn on_root_fs(w: &Where, path: &str) -> String {
    match &w.root_subvol {
        Some(sub) => format!("/{sub}{path}"),
        None => path.to_string(),
    }
}

/// A path under /boot as the boot loader and the live mount see it: from
/// the top of the filesystem /boot is on.
pub fn on_boot_fs(w: &Where, path: &str) -> String {
    if w.boot_apart { path.strip_prefix("/boot").unwrap_or(path).to_string() } else { on_root_fs(w, path) }
}

/// The kernel command line: the ISO's own options (from its loader entry)
/// plus where the image is, copytoram, the stage mount and the start script.
pub fn options(iso_options: &str, w: &Where, mode: Mode) -> String {
    let stage = on_boot_fs(w, STAGE);
    let mut mount_opts = vec!["ro"];
    // /boot inside a Btrfs root: mount the filesystem's top, where `stage` starts.
    let top;
    if !w.boot_apart && w.root_subvol.is_some() {
        top = "subvolid=5".to_string();
        mount_opts.push(&top);
    }
    let mut words: Vec<String> = iso_options
        .split_whitespace()
        .filter(|w| !w.starts_with("copytoram=") && !w.starts_with("script="))
        .map(String::from)
        .collect();
    words.extend([
        format!("img_dev=/dev/disk/by-uuid/{}", w.root_uuid),
        format!("img_loop={}", on_root_fs(w, IMAGE)),
        "copytoram=y".into(),
        format!("systemd.mount-extra=UUID={}:{LIVE_MOUNT}:{}:{}", w.boot_uuid, w.boot_fstype, mount_opts.join(",")),
        format!("script={LIVE_MOUNT}{stage}/start.sh"),
        match mode {
            Mode::Restore => "bindle.restore=1".into(),
            Mode::Rollback => "bindle.rollback=1".into(),
        },
    ]);
    words.join(" ")
}

/// systemd-boot's entry (loader/entries/ENTRY on the ESP).
pub fn loader_entry(w: &Where, options: &str, mode: Mode) -> String {
    let stage = on_boot_fs(w, STAGE);
    format!(
        "# Managed by Bindle\ntitle   {}\nlinux   {stage}/vmlinuz-linux\ninitrd  {stage}/initramfs-linux.img\noptions {options}\n",
        title(mode)
    )
}

fn title(mode: Mode) -> &'static str {
    match mode {
        Mode::Restore => "Bindle: Factory Restore",
        Mode::Rollback => "Bindle: Roll back the system",
    }
}

/// GRUB's environment variable that makes Factory Restore the next start's,
/// once (`grub-editenv set`). grub-reboot would need GRUB_DEFAULT=saved.
pub const GRUB_ONCE: &str = "bindle_restore";

/// GRUB's menu entries, as a /etc/grub.d script (grub-mkconfig runs it):
/// `entries` is each mode with its kernel options. Factory Restore's is the
/// default while GRUB_ONCE is set, which it clears as it starts.
pub fn grub_script(w: &Where, entries: &[(Mode, String)]) -> String {
    let stage = on_boot_fs(w, STAGE);
    let mut s = "#!/bin/sh\n# Managed by Bindle\nexec tail -n +4 \"$0\"\n".to_string();
    for (mode, options) in entries {
        let id = entry_id(*mode);
        if *mode == Mode::Restore {
            s.push_str(&format!(
                "if [ \"${{{GRUB_ONCE}}}\" = 1 ]; then\n    set default={id}\n    set {GRUB_ONCE}=\n    save_env {GRUB_ONCE}\nfi\n"
            ));
        }
        s.push_str(&format!(
            "menuentry '{}' --id {id} {{\n    search --no-floppy --fs-uuid --set=root {}\n    \
             linux {stage}/vmlinuz-linux {options}\n    initrd {stage}/initramfs-linux.img\n}}\n",
            title(*mode),
            w.boot_uuid
        ));
    }
    s
}

/// The start script archiso runs on the first console, for either mode
/// (the kernel command line says which). Bindle is copied into memory and
/// every mount of this computer's disk let go, so the installer may erase
/// it, or the rollback may change it.
pub fn start_script() -> String {
    format!(
        r#"#!/bin/sh
# Managed by Bindle
set -u
stage=$(dirname "$0")
cp "$stage/bindle" /root/bindle && chmod 755 /root/bindle
[ -f "$stage/bindle-gui" ] && cp "$stage/bindle-gui" /root/bindle-gui && chmod 755 /root/bindle-gui
umount {LIVE_MOUNT} 2>/dev/null
# copytoram: nothing needs the disk the image came from any more.
umount /run/archiso/img_dev 2>/dev/null
for l in $(losetup -nO NAME 2>/dev/null); do losetup -d "$l" 2>/dev/null; done
if grep -qw 'bindle.rollback=1' /proc/cmdline; then
    /root/bindle rollback --installed-system
else
    /root/bindle install
fi
echo
echo "Nothing more to do here. Restarting goes back to this computer's system."
printf 'Press Enter to restart. '
read -r _
systemctl reboot
"#
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    fn btrfs_esp() -> Where {
        Where {
            root_uuid: "r-1".into(),
            root_fstype: "btrfs".into(),
            root_subvol: Some("@".into()),
            boot_uuid: "B-1".into(),
            boot_fstype: "vfat".into(),
            boot_apart: true,
        }
    }

    const ISO: &str = "archisobasedir=arch archisosearchuuid=2026-11-01-08-42-25-00 copytoram=n";

    #[test]
    fn btrfs_root_with_an_esp() {
        let w = btrfs_esp();
        let o = options(ISO, &w, Mode::Restore);
        assert_eq!(
            o,
            "archisobasedir=arch archisosearchuuid=2026-11-01-08-42-25-00 img_dev=/dev/disk/by-uuid/r-1 \
             img_loop=/@/var/lib/bindle/restore/archlinux-x86_64.iso copytoram=y \
             systemd.mount-extra=UUID=B-1:/run/bindle-restore:vfat:ro script=/run/bindle-restore/bindle-restore/start.sh \
             bindle.restore=1"
        );
        let e = loader_entry(&w, &o, Mode::Restore);
        assert!(e.contains("linux   /bindle-restore/vmlinuz-linux\n") && e.contains("title   Bindle: Factory Restore"));
    }

    #[test]
    fn boot_inside_an_ext4_or_btrfs_root() {
        let ext4 = Where {
            root_fstype: "ext4".into(),
            root_subvol: None,
            boot_uuid: "r-1".into(),
            boot_fstype: "ext4".into(),
            boot_apart: false,
            ..btrfs_esp()
        };
        let o = options("", &ext4, Mode::Rollback);
        assert!(o.contains("img_loop=/var/lib/bindle/restore/archlinux-x86_64.iso"));
        assert!(o.contains("systemd.mount-extra=UUID=r-1:/run/bindle-restore:ext4:ro script=/run/bindle-restore/boot/bindle-restore/start.sh"));
        assert!(o.ends_with("bindle.rollback=1"));
        let btrfs = Where { boot_uuid: "r-1".into(), boot_fstype: "btrfs".into(), boot_apart: false, ..btrfs_esp() };
        let o = options("", &btrfs, Mode::Restore);
        assert!(o.contains(":btrfs:ro,subvolid=5 script=/run/bindle-restore/@/boot/bindle-restore/start.sh"), "{o}");
        let g = grub_script(&btrfs, &[(Mode::Restore, o)]);
        assert!(g.contains("linux /@/boot/bindle-restore/vmlinuz-linux ") && g.contains("--set=root r-1"));
    }

    #[test]
    fn scripts_are_valid_shell() {
        let s = start_script();
        assert!(std::process::Command::new("sh").args(["-n", "-c", &s]).status().unwrap().success(), "{s}");
        let g = grub_script(&btrfs_esp(), &[(Mode::Rollback, "r=1".into()), (Mode::Restore, "a=b".into())]);
        assert!(g.contains("menuentry 'Bindle: Roll back the system' --id bindle-rollback {"));
        // grub-mkconfig prints everything after the header lines.
        assert!(g.contains("\nif [ \"${bindle_restore}\" = 1 ]; then\n    set default=bindle-restore\n"));
        assert!(
            g.contains("    save_env bindle_restore\nfi\nmenuentry 'Bindle: Factory Restore' --id bindle-restore {")
        );
    }
}
