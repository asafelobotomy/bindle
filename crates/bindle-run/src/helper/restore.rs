//! Factory Restore's preparation, as root: check the Arch ISO's signature
//! (pacman's own Arch keyring), put it on the root filesystem, put its
//! kernel, a copy of Bindle and the start script on /boot, add a boot entry
//! and make it the next start's, once. Everything is worked out here from
//! the system itself; the client names only the downloaded ISO and its
//! signature. See bindle_core::recover::restore.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use bindle_core::recover::restore::{self, IMAGE, Mode, STAGE, Where};

use super::Helper;
use crate::sys::{check_system_path, host};

/// pacman's keyring of Arch's developers (archlinux-keyring), and the keys
/// it no longer trusts.
const KEYRING: &str = "/usr/share/pacman/keyrings/archlinux.gpg";
const REVOKED: &str = "/usr/share/pacman/keyrings/archlinux-revoked";
const GRUB_SCRIPT: &str = "/etc/grub.d/42_bindle_restore";

fn output(argv: &[&str]) -> Result<String> {
    let out = Command::new(argv[0]).args(&argv[1..]).output().with_context(|| format!("start {}", argv[0]))?;
    if !out.status.success() {
        bail!("{} failed: {}", argv.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The ISO carries a good signature by a key of Arch's that isn't revoked.
pub fn verify_iso(iso: &Path, sig: &Path) -> Result<()> {
    let out = Command::new("gpgv")
        .args(["--status-fd", "1", "--keyring", KEYRING])
        .arg(sig)
        .arg(iso)
        .output()
        .context("start gpgv")?;
    let status = String::from_utf8_lossy(&out.stdout);
    let fpr = status
        .lines()
        .find_map(|l| l.strip_prefix("[GNUPG:] VALIDSIG "))
        .and_then(|rest| rest.split_whitespace().next())
        .ok_or_else(|| anyhow!("the ISO's signature isn't a good one by a key of Arch's"))?;
    let revoked = fs::read_to_string(REVOKED).unwrap_or_default();
    if revoked.lines().any(|l| l.trim().eq_ignore_ascii_case(fpr)) {
        bail!("the ISO is signed with a key Arch has revoked ({fpr})");
    }
    Ok(())
}

/// Where root and /boot are, from the mount table and blkid.
fn locate(mountinfo: &str) -> Result<Where> {
    let at = |point: &str| {
        mountinfo
            .lines()
            .filter_map(|l| {
                let (left, right) = l.split_once(" - ")?;
                let left: Vec<&str> = left.split_whitespace().collect();
                let mut right = right.split_whitespace();
                let (fstype, source) = (right.next()?, right.next()?);
                (left.get(4)? == &point).then(|| (left[3].to_string(), fstype.to_string(), source.to_string()))
            })
            .next_back()
    };
    let (root_sub, root_fstype, root_dev) = at("/").ok_or_else(|| anyhow!("/ isn't in the mount table"))?;
    if root_dev.starts_with("/dev/mapper/") || root_dev.starts_with("/dev/dm-") {
        bail!(
            "the system is encrypted: the installer can't start from a file inside it. Use a USB stick instead (see `bindle iso`)."
        );
    }
    let uuid = |dev: &str| output(&["blkid", "-s", "UUID", "-o", "value", dev]).map(|s| s.trim().to_string());
    let root_subvol =
        (root_fstype == "btrfs").then(|| root_sub.trim_start_matches('/').to_string()).filter(|s| !s.is_empty());
    let root_uuid = uuid(&root_dev)?;
    let (boot_uuid, boot_fstype, boot_apart) = match at("/boot") {
        Some((_, fstype, dev)) if dev != root_dev => (uuid(&dev)?, fstype, true),
        _ => (root_uuid.clone(), root_fstype.clone(), false),
    };
    Ok(Where { root_uuid, root_fstype, root_subvol, boot_uuid, boot_fstype, boot_apart })
}

impl Helper {
    /// The ISO on the root filesystem (`IMAGE`, unless it is that already),
    /// then checked there: the user's copy could change after a check,
    /// root's can't. Kept for Factory Restore without the network.
    pub(super) fn store_image(&mut self, iso: &str, sig: &str) -> Result<std::path::PathBuf> {
        check_system_path(iso)?;
        check_system_path(sig)?;
        let fake = self.opts.fake_commands;
        let image = host(self.root(), IMAGE);
        let image_sig = image.with_extension("iso.sig");
        fs::create_dir_all(image.parent().expect("has a parent"))?;
        if Path::new(iso) != image {
            self.output(&format!("Copying the image to {IMAGE}"));
            if !fake {
                fs::copy(iso, &image).with_context(|| format!("copy {iso} to {IMAGE}"))?;
                fs::copy(sig, &image_sig)?;
            }
        }
        if fake {
            self.output(&format!("would check {IMAGE} against its signature with {KEYRING}"));
        } else if let Err(e) = verify_iso(&image, &image_sig) {
            let _ = fs::remove_file(&image);
            let _ = fs::remove_file(&image_sig);
            return Err(e);
        }
        if !fake {
            self.output("The ISO's signature is good.");
        }
        Ok(image)
    }

    /// The boot loader and where things are; refuses what can't work.
    fn setup(&mut self) -> Result<(String, Where)> {
        let loader = bindle_core::Facts::detect().bootloader;
        if !matches!(loader.as_str(), "systemd-boot" | "grub") {
            bail!("this works with systemd-boot and GRUB; this system starts with {loader}");
        }
        let info = fs::read_to_string("/proc/self/mountinfo").context("read the mount table")?;
        let w = locate(&info)?;
        if loader == "grub" && w.boot_fstype == "btrfs" {
            // GRUB can't write its environment there, so "once" would be "always".
            bail!(
                "GRUB can't mark a one-time start on Btrfs (/boot is on it here); use a USB stick instead (see `bindle iso`)"
            );
        }
        Ok((loader, w))
    }

    /// The ISO's kernel and initramfs, a copy of Bindle and the start script
    /// on /boot; the ISO's own kernel options.
    fn stage(&mut self, image: &Path) -> Result<String> {
        let stage = host(self.root(), STAGE);
        fs::create_dir_all(&stage)?;
        let iso_options = if self.opts.fake_commands {
            String::new()
        } else {
            let kernel = [
                ("arch/boot/x86_64/vmlinuz-linux", "vmlinuz-linux"),
                ("arch/boot/x86_64/initramfs-linux.img", "initramfs-linux.img"),
            ];
            for (inside, name) in kernel {
                let out = Command::new("bsdtar").arg("-xOf").arg(image).arg(inside).output().context("start bsdtar")?;
                if !out.status.success() || out.stdout.is_empty() {
                    bail!("the ISO has no {inside}");
                }
                fs::write(stage.join(name), out.stdout)?;
            }
            let entry =
                output(&["bsdtar", "-xOf", &image.display().to_string(), "loader/entries/01-archiso-linux.conf"])?;
            entry.lines().find_map(|l| l.strip_prefix("options")).map(|o| o.trim().to_string()).unwrap_or_default()
        };
        let exe = std::env::current_exe().context("find Bindle's own program")?;
        fs::copy(&exe, stage.join("bindle")).context("copy Bindle")?;
        if let Some(gui) = exe.parent().map(|d| d.join("bindle-gui")).filter(|g| g.is_file()) {
            let _ = fs::copy(gui, stage.join("bindle-gui"));
        }
        fs::write(stage.join("start.sh"), restore::start_script())?;
        fs::write(stage.join("options"), &iso_options)?;
        Ok(iso_options)
    }

    fn esp(&self) -> Result<String> {
        if self.opts.fake_commands {
            return Ok("/boot".into());
        }
        Ok(output(&["bootctl", "--print-esp-path"])?.trim().to_string())
    }

    /// The boot entries: Factory Restore's (`restore`) and the menu's
    /// Rollback (`rollback`), each written or taken away.
    fn entries(&mut self, loader: &str, w: &Where, restore_on: bool, rollback_on: bool) -> Result<()> {
        let iso_options = fs::read_to_string(host(self.root(), &format!("{STAGE}/options"))).unwrap_or_default();
        let wanted: Vec<(Mode, String)> = [(Mode::Rollback, rollback_on), (Mode::Restore, restore_on)]
            .into_iter()
            .filter(|(_, on)| *on)
            .map(|(m, _)| (m, restore::options(&iso_options, w, m)))
            .collect();
        if loader == "systemd-boot" {
            let dir = host(self.root(), &format!("{}/loader/entries", self.esp()?));
            fs::create_dir_all(&dir)?;
            for mode in [Mode::Restore, Mode::Rollback] {
                let file = dir.join(format!("{}.conf", restore::entry_id(mode)));
                match wanted.iter().find(|(m, _)| *m == mode) {
                    Some((_, options)) => fs::write(&file, restore::loader_entry(w, options, mode))?,
                    None => drop(fs::remove_file(&file)),
                }
            }
            return Ok(());
        }
        let script = host(self.root(), GRUB_SCRIPT);
        if wanted.is_empty() {
            let _ = fs::remove_file(&script);
        } else {
            fs::create_dir_all(script.parent().expect("has a parent"))?;
            fs::write(&script, restore::grub_script(w, &wanted))?;
            fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
        }
        self.cmd(&["grub-mkconfig", "-o", "/boot/grub/grub.cfg"])
    }

    /// Is this mode's entry there now?
    fn entry_on(&self, mode: Mode) -> bool {
        let id = restore::entry_id(mode);
        let grub = fs::read_to_string(host(self.root(), GRUB_SCRIPT)).is_ok_and(|s| s.contains(&format!("--id {id} ")));
        let sdboot = ["/boot", "/efi", "/boot/efi"]
            .iter()
            .any(|esp| host(self.root(), &format!("{esp}/loader/entries/{id}.conf")).is_file());
        grub || sdboot
    }

    /// Rolling back from the boot menu works here: Btrfs and snapper.
    fn rollback_possible(&self, w: &Where) -> bool {
        w.root_fstype == "btrfs" && w.root_subvol.is_some() && host(self.root(), "/etc/snapper/configs/root").is_file()
    }

    /// Factory Restore: the next start, once, is the installer.
    pub(super) fn prepare_restore(&mut self, iso: &str, sig: &str) -> Result<()> {
        let (loader, w) = self.setup()?;
        let image = self.store_image(iso, sig)?;
        self.stage(&image)?;
        let rollback_on = self.entry_on(Mode::Rollback);
        self.entries(&loader, &w, true, rollback_on)?;
        if loader == "systemd-boot" {
            let entry = format!("{}.conf", restore::entry_id(Mode::Restore));
            self.cmd(&["bootctl", "set-oneshot", &entry])?;
        } else {
            let once = format!("{}=1", restore::GRUB_ONCE);
            self.cmd(&["grub-editenv", "/boot/grub/grubenv", "set", &once])?;
        }
        self.output("The next start opens the installer. Restart when you're ready.");
        Ok(())
    }

    /// Keep a checked image, and (Btrfs with snapper) a "Roll back the
    /// system" entry in the boot menu for when the system won't start.
    pub(super) fn keep_image(&mut self, iso: &str, sig: &str) -> Result<()> {
        let (loader, w) = self.setup()?;
        let image = self.store_image(iso, sig)?;
        self.stage(&image)?;
        let rollback_on = self.rollback_possible(&w);
        let restore_on = self.entry_on(Mode::Restore);
        self.entries(&loader, &w, restore_on, rollback_on)?;
        if rollback_on {
            self.output("The boot menu has \"Bindle: Roll back the system\" for when the system won't start.");
        }
        Ok(())
    }

    /// Factory Restore called off; with `keep_image` false, the kept image,
    /// the staged files and the menu's Rollback go too.
    pub(super) fn cancel_restore(&mut self, keep_image: bool) -> Result<()> {
        let (loader, w) = self.setup()?;
        let rollback_on = keep_image && self.entry_on(Mode::Rollback);
        self.entries(&loader, &w, false, rollback_on)?;
        if loader == "grub" {
            self.cmd(&["grub-editenv", "/boot/grub/grubenv", "unset", restore::GRUB_ONCE])?;
        }
        if !keep_image {
            let _ = fs::remove_dir_all(host(self.root(), STAGE));
            let image = host(self.root(), IMAGE);
            let _ = fs::remove_file(&image);
            let _ = fs::remove_file(image.with_extension("iso.sig"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_encrypted_root_is_refused_before_anything_runs() {
        let info = "33 2 0:30 /@ / rw - btrfs /dev/mapper/root rw\n";
        assert!(locate(info).unwrap_err().to_string().contains("encrypted"));
        assert!(locate("").is_err());
    }
}
