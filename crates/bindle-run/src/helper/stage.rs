//! Staging AUR packages built as the user before `pacman -U` reads them.

use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::{Helper, STAGING};
use crate::sys::{check_system_path, create_dirs, host, write_atomic};

impl Helper {
    /// Copy user-built package files into a root-owned directory, so they
    /// can't change between the checks here and pacman reading them.
    pub(super) fn stage_packages(&self, paths: &[String]) -> Result<Vec<String>> {
        let dir = host(self.root(), STAGING);
        let _ = fs::remove_dir_all(&dir);
        create_dirs(&dir)?;
        let mut out = Vec::new();
        for p in paths {
            check_system_path(p)?;
            let name = Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !name.contains(".pkg.tar") || name.starts_with('.') {
                bail!("not a package file: {p}");
            }
            // Check and read the same open file: never through a symlink, and
            // O_NONBLOCK so a FIFO planted at the path can't hang the helper.
            let mut file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(p)
                .map_err(|_| anyhow::anyhow!("not a package file: {p}"))?;
            if !file.metadata().is_ok_and(|m| m.is_file()) {
                bail!("not a package file: {p}");
            }
            let mut data = Vec::new();
            file.read_to_end(&mut data).with_context(|| format!("read {p}"))?;
            write_atomic(&dir.join(&name), &data, 0o644)?;
            out.push(format!("{STAGING}/{name}"));
        }
        Ok(out)
    }
}
