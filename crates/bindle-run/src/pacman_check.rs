//! Checks the root helper makes before it edits pacman.conf, and the one
//! listing it gives back: the files in /boot, which is often root-only.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Result, bail};

use crate::sys::{host, write_atomic};

/// pacman.conf `[options]` keys are single words; values are one line. Anything
/// else could inject sections or directives.
pub fn edit(options: &BTreeMap<String, String>, repos: &[String]) -> Result<()> {
    for (k, v) in options {
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric()) || v.contains(['\n', '\r', '[', ']']) {
            bail!("refusing pacman.conf option `{k}`");
        }
    }
    match repos.iter().find(|r| r.is_empty() || !r.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
        Some(r) => bail!("refusing repo name `{r}`"),
        None => Ok(()),
    }
}

/// Let pacman itself parse the edited file before it replaces the real one.
pub fn conf(text: &str) -> Result<()> {
    let tmp = Path::new("/etc/.pacman.conf.bindle-check");
    write_atomic(tmp, text.as_bytes(), 0o600)?;
    let out = std::process::Command::new("pacman-conf").arg("--config").arg(tmp).arg("--repo-list").output();
    let _ = fs::remove_file(tmp);
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            bail!("edited pacman.conf does not parse; left unchanged: {}", String::from_utf8_lossy(&o.stderr).trim())
        }
        // No pacman-conf (shouldn't happen on Arch): nothing to check with.
        Err(_) => Ok(()),
    }
}

/// The names of the files in /boot (no contents), for bindleupgrade's check
/// that every kernel has its boot images.
pub fn boot_files(root: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> =
        fs::read_dir(host(root, "/boot"))?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.retain(|n| !n.contains(['\n', '\r']));
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_lists_names_only() {
        let root = crate::sys::test_dir("boot-files");
        fs::create_dir_all(root.join("boot")).unwrap();
        fs::write(root.join("boot/vmlinuz-linux-zen"), "kernel").unwrap();
        fs::write(root.join("boot/initramfs-linux-zen.img"), "").unwrap();
        assert_eq!(boot_files(&root).unwrap(), ["initramfs-linux-zen.img", "vmlinuz-linux-zen"]);
        assert!(boot_files(&root.join("nothing")).is_err());
    }
}
