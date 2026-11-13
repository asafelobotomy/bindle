//! The initramfs side of NVIDIA's own driver: Bindle's mkinitcpio drop-in
//! (see `crate::initramfs`), written while the driver is in use or being
//! installed and removed again once the machine is back on nouveau.

use super::Expansion;
use crate::catalog::FileSpec;
use crate::detect::Facts;
use crate::initramfs::{NVIDIA_DROPIN, nvidia_dropin};

/// The integrated GPU's kernel module on a hybrid machine: the bound driver,
/// else the usual one for its vendor.
fn igpu_module(f: &Facts) -> Option<String> {
    if !f.hybrid_gpu() {
        return None;
    }
    let bound = f.igpu_driver.as_deref().filter(|d| matches!(*d, "i915" | "xe" | "amdgpu" | "radeon"));
    let fallback = if f.gpus.iter().any(|g| g == "intel") { "i915" } else { "amdgpu" };
    Some(bound.unwrap_or(fallback).to_string())
}

/// `nvidia_driver`: NVIDIA's own driver is installed or this plan installs it.
pub(super) fn nvidia(f: &Facts, nvidia_driver: bool, e: &mut Expansion) {
    let Some(m) = &f.mkinitcpio else { return };
    let wanted = if nvidia_driver { nvidia_dropin(m, igpu_module(f).as_deref()) } else { None };
    match wanted {
        Some(content) => {
            if m.dropin.as_deref() != Some(content.as_str()) {
                e.rebuild_initramfs = true;
                e.notes.push(format!(
                    "NVIDIA's driver: nouveau and its firmware are left out of the initramfs ({NVIDIA_DROPIN} \
                     drops the kms hook), so the boot image is smaller. Check with \
                     `lsinitcpio -a /boot/initramfs-linux.img | grep nouveau` (nothing found)."
                ));
            }
            e.files.push(FileSpec { path: NVIDIA_DROPIN.into(), mode: 0o644, content });
        }
        // Back on nouveau (or mkinitcpio.conf now has it all): the drop-in goes.
        None if m.dropin_is_ours() => {
            e.remove_files.push(NVIDIA_DROPIN.into());
            e.rebuild_initramfs = true;
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Ctx, expand};
    use super::*;
    use crate::initramfs::Mkinitcpio;

    const HOOKS: &str = "HOOKS=(base udev autodetect microcode modconf kms keyboard block filesystems fsck)\n";

    fn facts(gpus: &[&str], devices: &[u16], dropin: Option<&str>) -> Facts {
        let ours = NVIDIA_DROPIN.rsplit('/').next().unwrap().to_string();
        let files: Vec<(String, String)> = dropin.map(|d| (ours, d.to_string())).into_iter().collect();
        Facts {
            gpus: gpus.iter().map(|g| g.to_string()).collect(),
            nvidia_devices: devices.to_vec(),
            mkinitcpio: Mkinitcpio::from_files(HOOKS, &files),
            ..Default::default()
        }
    }

    fn written(e: &Expansion) -> Option<&str> {
        e.files.iter().find(|f| f.path == NVIDIA_DROPIN).map(|f| f.content.as_str())
    }

    #[test]
    fn nvidia_open_drops_kms_and_keeps_the_intel_gpu_early() {
        let mut f = facts(&["intel", "nvidia"], &[0x28e0], None);
        f.igpu_driver = Some("xe".into());
        let e = expand("gpu", &Ctx { facts: &f, multilib: false });
        assert!(e.packages.contains(&"nvidia-open-dkms".to_string()));
        let d = written(&e).expect("drop-in");
        assert!(d.contains("HOOKS=(base udev autodetect microcode modconf keyboard block filesystems fsck)"), "{d}");
        assert!(d.contains("MODULES=(xe)"), "{d}");
        assert!(e.rebuild_initramfs && e.notes.iter().any(|n| n.contains("lsinitcpio")));
        // Desktop card, no integrated GPU: HOOKS only. AMD iGPU without a bound driver: amdgpu.
        let e = expand("gpu", &Ctx { facts: &facts(&["nvidia"], &[0x2684], None), multilib: false });
        assert!(!written(&e).unwrap().contains("MODULES"));
        let e = expand("gpu", &Ctx { facts: &facts(&["amd", "nvidia"], &[0x2684], None), multilib: false });
        assert!(written(&e).unwrap().contains("MODULES=(amdgpu)"));
    }

    #[test]
    fn installed_and_legacy_drivers_get_it_too() {
        let mut f = facts(&["intel", "nvidia"], &[0x1c8d], None);
        f.installed.insert("nvidia-580xx-dkms".into());
        let e = expand("gpu", &Ctx { facts: &f, multilib: false });
        assert!(written(&e).unwrap().contains("MODULES=(i915)"));
        let f = facts(&["nvidia"], &[0x1b80], None);
        let e = expand("nvidia-legacy", &Ctx { facts: &f, multilib: false });
        assert!(written(&e).is_some() && e.rebuild_initramfs);
    }

    #[test]
    fn unchanged_drop_in_needs_no_rebuild() {
        let first = expand("gpu", &Ctx { facts: &facts(&["nvidia"], &[0x2684], None), multilib: false });
        let d = written(&first).unwrap().to_string();
        let e = expand("gpu", &Ctx { facts: &facts(&["nvidia"], &[0x2684], Some(&d)), multilib: false });
        assert_eq!(written(&e), Some(d.as_str()));
        assert!(!e.rebuild_initramfs && e.remove_files.is_empty());
    }

    #[test]
    fn nouveau_removes_bindles_drop_in_only() {
        let ours = "# Managed by Bindle\nHOOKS=(base udev)\n";
        // Pre-Turing card without NVIDIA's driver: nouveau, so the drop-in goes.
        let e = expand("gpu", &Ctx { facts: &facts(&["nvidia"], &[0x1b80], Some(ours)), multilib: false });
        assert!(written(&e).is_none());
        assert_eq!(e.remove_files, [NVIDIA_DROPIN]);
        assert!(e.rebuild_initramfs);
        // Someone else's file of that name stays; no drop-in at all: nothing.
        let e = expand("gpu", &Ctx { facts: &facts(&["nvidia"], &[0x1b80], Some("HOOKS=(x)\n")), multilib: false });
        assert!(e.remove_files.is_empty() && !e.rebuild_initramfs);
        let e = expand("gpu", &Ctx { facts: &facts(&["amd"], &[], None), multilib: false });
        assert!(e.files.is_empty() && e.remove_files.is_empty() && !e.rebuild_initramfs);
        // No mkinitcpio (dracut, booster): nothing either.
        let f = Facts { gpus: vec!["nvidia".into()], nvidia_devices: vec![0x2684], ..Default::default() };
        let e = expand("gpu", &Ctx { facts: &f, multilib: false });
        assert!(e.files.is_empty() && !e.rebuild_initramfs);
    }

    #[test]
    fn plan_writes_or_removes_it_and_rebuilds() {
        use crate::Catalog;
        use crate::plan::{Selection, Step, resolve};
        let old = "# Managed by Bindle\nHOOKS=(base udev)\n";
        let mut f = facts(&["nvidia"], &[0x1b80], Some(old)); // GTX 1080: the 580 driver, from the AUR
        f.distro_id = "arch".into();
        let current = |p: &str| (p == NVIDIA_DROPIN).then(|| old.to_string());
        let run = |sel: Selection| resolve(&Catalog::arch().unwrap(), &f, &sel, &current).unwrap().steps;
        let removes = |s: &Step| matches!(s, Step::RemoveFiles { paths } if paths == &[NVIDIA_DROPIN]);
        let writes = |s: &Step| matches!(s, Step::WriteFile { file, .. } if file.path == NVIDIA_DROPIN);
        // gpu-driver alone means nouveau here, but nvidia-legacy installs the 580 driver: written, kept.
        let steps = run(Selection::default());
        assert!(steps.iter().any(writes) && !steps.iter().any(removes), "{steps:?}");
        assert!(steps.contains(&Step::RebuildInitramfs));
        let rebuild = steps.iter().position(|s| *s == Step::RebuildInitramfs).unwrap();
        assert!(steps.iter().position(writes).unwrap() < rebuild);
        // Without it, back on nouveau: Bindle's drop-in goes and the images are rebuilt.
        let steps = run(Selection { exclude: vec!["nvidia-legacy".into()], ..Default::default() });
        assert!(steps.iter().any(removes) && !steps.iter().any(writes), "{steps:?}");
        assert!(steps.contains(&Step::RebuildInitramfs));
    }
}
