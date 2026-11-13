//! mkinitcpio's configuration, read without root, and Bindle's drop-in for
//! NVIDIA's own driver.
//!
//! With NVIDIA's driver installed, the `kms` hook still puts nouveau (autodetect
//! resolves the NVIDIA card's modalias to it) and every NVIDIA firmware file
//! nouveau declares (over 100 MiB) into the initramfs, and nouveau can load in
//! early boot. The Arch wiki (NVIDIA, "Early loading") says to drop `kms` from
//! HOOKS; on hybrid laptops the integrated GPU's driver goes into MODULES to keep
//! its early KMS. Bindle does both in a drop-in instead of editing
//! mkinitcpio.conf.

use std::fs;

use serde::Serialize;

use crate::plan::retire::MARKER;

pub const CONF: &str = "/etc/mkinitcpio.conf";
pub const CONF_DIR: &str = "/etc/mkinitcpio.conf.d";
/// mkinitcpio sources conf.d files in (version) sort order after
/// mkinitcpio.conf, so `zz-` puts Bindle's HOOKS last, where they win.
pub const NVIDIA_DROPIN: &str = "/etc/mkinitcpio.conf.d/zz-bindle-nvidia.conf";

/// What mkinitcpio would use without Bindle's drop-in, and the drop-in now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Mkinitcpio {
    pub hooks: Vec<String>,
    pub modules: Vec<String>,
    /// Bindle's drop-in (`NVIDIA_DROPIN`) as it is now.
    pub dropin: Option<String>,
}

impl Mkinitcpio {
    /// `main` is mkinitcpio.conf; `dropins` are conf.d's (name, text) pairs,
    /// Bindle's own among them or not. None without HOOKS (not mkinitcpio's).
    pub fn from_files(main: &str, dropins: &[(String, String)]) -> Option<Self> {
        let ours = NVIDIA_DROPIN.rsplit('/').next().unwrap_or_default();
        let mut sorted: Vec<&(String, String)> = dropins.iter().filter(|(n, _)| n.ends_with(".conf")).collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let mut text = main.to_string();
        for (_, t) in sorted.iter().filter(|(n, _)| n != ours) {
            text.push('\n');
            text.push_str(t);
        }
        Some(Mkinitcpio {
            hooks: array(&text, "HOOKS")?,
            modules: array(&text, "MODULES").unwrap_or_default(),
            dropin: sorted.iter().find(|(n, _)| n == ours).map(|(_, t)| t.clone()),
        })
    }

    pub fn detect() -> Option<Self> {
        let main = fs::read_to_string(CONF).ok()?;
        let dropins: Vec<(String, String)> = fs::read_dir(CONF_DIR)
            .map(|d| {
                d.flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        fs::read_to_string(e.path()).ok().map(|t| (name, t))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self::from_files(&main, &dropins)
    }

    /// Bindle wrote the drop-in that is there now.
    pub fn dropin_is_ours(&self) -> bool {
        self.dropin.as_deref().is_some_and(|d| d.starts_with(MARKER))
    }
}

/// The values of a bash array (`NAME=(a b)`, over several lines too, or the old
/// string form `NAME="a b"`), as the last assignment leaves it; `NAME+=(…)`
/// appends. None when it is never assigned.
pub fn array(text: &str, name: &str) -> Option<Vec<String>> {
    let mut out: Option<Vec<String>> = None;
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let line = strip_comment(line).trim_start();
        let Some(rest) = line.strip_prefix(name) else { continue };
        let (append, rest) = match (rest.strip_prefix("+="), rest.strip_prefix('=')) {
            (Some(r), _) => (true, r),
            (None, Some(r)) => (false, r),
            _ => continue,
        };
        let values = if let Some(body) = rest.strip_prefix('(') {
            let mut body = body.to_string();
            while !body.contains(')') {
                let Some(next) = lines.next() else { break };
                body.push(' ');
                body.push_str(strip_comment(next));
            }
            words(body.split(')').next().unwrap_or_default())
        } else if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
            words(rest[1..].split(q).next().unwrap_or_default())
        } else {
            words(rest.split_whitespace().next().unwrap_or_default())
        };
        match (&mut out, append) {
            (Some(v), true) => v.extend(values),
            _ => out = Some(values),
        }
    }
    out
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(|w| w.trim_matches(['"', '\''])).filter(|w| !w.is_empty()).map(String::from).collect()
}

/// A line without its `#` comment (a `#` at the start or after a space).
fn strip_comment(line: &str) -> &str {
    let b = line.as_bytes();
    match (0..b.len()).find(|&i| b[i] == b'#' && (i == 0 || b[i - 1].is_ascii_whitespace())) {
        Some(i) => &line[..i],
        None => line,
    }
}

/// The drop-in for NVIDIA's own driver: HOOKS without `kms`, and MODULES with
/// `igpu` (the integrated GPU's driver, on hybrid machines). None when the
/// configuration already has both.
pub fn nvidia_dropin(m: &Mkinitcpio, igpu: Option<&str>) -> Option<String> {
    let kms = m.hooks.iter().any(|h| h == "kms");
    let add = igpu.filter(|i| !m.modules.iter().any(|x| x == i));
    if !kms && add.is_none() {
        return None;
    }
    let mut s = format!(
        "{MARKER}\n# NVIDIA's own driver is in use: without the kms hook, nouveau and the NVIDIA firmware it\n\
         # declares stay out of the initramfs (Arch wiki: NVIDIA, Early loading).\n"
    );
    if let Some(i) = add {
        s.push_str(&format!("# {i} keeps early KMS for the integrated GPU.\n"));
    }
    if kms {
        s.push_str("# HOOKS is mkinitcpio.conf's, copied when Bindle wrote this: change hooks here too.\n");
        let hooks: Vec<&str> = m.hooks.iter().map(String::as_str).filter(|h| *h != "kms").collect();
        s.push_str(&format!("HOOKS=({})\n", hooks.join(" ")));
    }
    if let Some(i) = add {
        let modules: Vec<&str> = m.modules.iter().map(String::as_str).chain([i]).collect();
        s.push_str(&format!("MODULES=({})\n", modules.join(" ")));
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCH: &str = "# vim:set ft=sh\nMODULES=()\nBINARIES=()\nFILES=()\n\
        # HOOKS=(base udev)\n\
        HOOKS=(base udev autodetect microcode modconf kms keyboard keymap consolefont block filesystems fsck)\n\
        #COMPRESSION=\"zstd\"\n";

    fn conf(files: &[(&str, &str)]) -> Vec<(String, String)> {
        files.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect()
    }

    #[test]
    fn reads_bash_arrays() {
        assert_eq!(array(ARCH, "HOOKS").unwrap()[5], "kms");
        assert_eq!(array(ARCH, "MODULES"), Some(vec![]));
        assert_eq!(array(ARCH, "COMPRESSION"), None);
        let multi = "HOOKS=(base # first\n  udev\n  kms)\nMODULES=\"i915 nvme\"\nMODULES+=(btrfs)\n";
        assert_eq!(array(multi, "HOOKS").unwrap(), ["base", "udev", "kms"]);
        assert_eq!(array(multi, "MODULES").unwrap(), ["i915", "nvme", "btrfs"]);
        assert_eq!(array("MODULES_DECOMPRESS=\"no\"\n", "MODULES"), None);
        assert_eq!(array("HOOKS=(a)\nHOOKS=(b c)\n", "HOOKS").unwrap(), ["b", "c"]);
    }

    #[test]
    fn dropins_override_in_order_without_bindles_own() {
        let ours = NVIDIA_DROPIN.rsplit('/').next().unwrap();
        let files = conf(&[
            ("20-later.conf", "HOOKS=(base systemd kms)\n"),
            ("10-first.conf", "HOOKS=(base udev)\nMODULES=(nvme)\n"),
            ("notes.txt", "HOOKS=(ignored)\n"),
            (ours, "# Managed by Bindle\nHOOKS=(base)\n"),
        ]);
        let m = Mkinitcpio::from_files(ARCH, &files).unwrap();
        assert_eq!(m.hooks, ["base", "systemd", "kms"]);
        assert_eq!(m.modules, ["nvme"]);
        assert!(m.dropin_is_ours());
        assert!(Mkinitcpio::from_files("# booster\n", &[]).is_none());
    }

    #[test]
    fn dropin_drops_kms_and_adds_the_igpu_driver() {
        let m = Mkinitcpio::from_files(ARCH, &[]).unwrap();
        let d = nvidia_dropin(&m, Some("i915")).unwrap();
        assert!(d.starts_with(MARKER));
        assert!(
            d.lines().any(|l| l
                == "HOOKS=(base udev autodetect microcode modconf keyboard keymap consolefont block filesystems fsck)"),
            "{d}"
        );
        assert!(d.lines().any(|l| l == "MODULES=(i915)"), "{d}");
        // What mkinitcpio reads with the drop-in in place: no kms, i915 loaded early.
        let after = Mkinitcpio::from_files(ARCH, &conf(&[("x.conf", &d)])).unwrap();
        assert!(!after.hooks.contains(&"kms".to_string()) && after.modules == ["i915"]);
        // No integrated GPU: HOOKS only.
        let d = nvidia_dropin(&m, None).unwrap();
        assert!(!d.contains("MODULES"));
    }

    #[test]
    fn nothing_to_do_once_the_configuration_has_it() {
        let fixed =
            conf(&[("10-nvidia.conf", "HOOKS=(base udev autodetect modconf block filesystems)\nMODULES=(i915)\n")]);
        let m = Mkinitcpio::from_files(ARCH, &fixed).unwrap();
        assert_eq!(nvidia_dropin(&m, Some("i915")), None);
        // kms already gone, the iGPU driver missing: MODULES only.
        let d = nvidia_dropin(&m, Some("amdgpu")).unwrap();
        assert!(!d.contains("HOOKS=") && d.contains("MODULES=(i915 amdgpu)"), "{d}");
    }
}
