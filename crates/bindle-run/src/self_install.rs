//! The root helper's side of Bindle updating itself: only Bindle's own
//! programs in /usr/local/bin are replaced, each from a root-owned copy of
//! the user's download whose sha256 must match the one the signed
//! SHA256SUMS lists (checked on the copy, so the file can't change between
//! the check and the install).

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::protocol::BindleFile;
use crate::sys::{check_system_path, create_dirs, host, write_atomic};

/// The only files this replaces.
pub const DESTS: [&str; 2] = ["/usr/local/bin/bindle", "/usr/local/bin/bindle-gui"];
const STAGING: &str = "/var/cache/bindle/self";

pub fn check(f: &BindleFile) -> Result<()> {
    if !DESTS.contains(&f.dest.as_str()) {
        bail!("refusing to install Bindle to {}", f.dest);
    }
    check_system_path(&f.src)?;
    if f.sha256.len() != 64 || !f.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("refusing a malformed checksum for {}", f.dest);
    }
    Ok(())
}

pub fn install(root: &Path, files: &[BindleFile], fake: bool, out: &mut dyn FnMut(&str)) -> Result<()> {
    for f in files {
        check(f)?;
    }
    if fake {
        files.iter().for_each(|f| out(&format!("would install {} as {} (sha256 {})", f.src, f.dest, f.sha256)));
        return Ok(());
    }
    let staging = host(root, STAGING);
    let _ = fs::remove_dir_all(&staging);
    create_dirs(&staging)?;
    for f in files {
        let regular = fs::symlink_metadata(&f.src).is_ok_and(|m| m.is_file());
        if !regular {
            bail!("not a file: {}", f.src);
        }
        let data = fs::read(&f.src).with_context(|| format!("read {}", f.src))?;
        let copy = staging.join(Path::new(&f.dest).file_name().context("file name")?);
        write_atomic(&copy, &data, 0o700)?;
        let sum = Command::new("sha256sum").arg("--").arg(&copy).output().context("run sha256sum")?;
        let got = String::from_utf8_lossy(&sum.stdout).split_whitespace().next().unwrap_or_default().to_lowercase();
        if got != f.sha256.to_lowercase() {
            bail!("{} doesn't match its signed checksum; nothing replaced", f.src);
        }
        write_atomic(&host(root, &f.dest), &data, 0o755)?;
        out(&format!("installed {}", f.dest));
    }
    let _ = fs::remove_dir_all(&staging);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(dest: &str, sha: &str) -> BindleFile {
        BindleFile { src: "/home/u/.cache/bindle/self/v1/bindle-x86_64".into(), dest: dest.into(), sha256: sha.into() }
    }

    #[test]
    fn only_bindles_own_programs() {
        assert!(check(&file("/usr/local/bin/bindle", &"a".repeat(64))).is_ok());
        assert!(check(&file("/usr/bin/sudo", &"a".repeat(64))).is_err());
        assert!(check(&file("/usr/local/bin/bindle", "abc")).is_err());
    }

    #[test]
    fn a_wrong_checksum_replaces_nothing() {
        let root = crate::sys::test_dir("self-install");
        let src = root.join("download");
        fs::write(&src, b"new bindle").unwrap();
        let f =
            BindleFile { src: src.display().to_string(), dest: "/usr/local/bin/bindle".into(), sha256: "0".repeat(64) };
        let e = install(&root, std::slice::from_ref(&f), false, &mut |_| {}).unwrap_err();
        assert!(e.to_string().contains("doesn't match"));
        assert!(!host(&root, "/usr/local/bin/bindle").exists());
        let good = String::from_utf8(Command::new("sha256sum").arg(&src).output().unwrap().stdout).unwrap();
        let f = BindleFile { sha256: good.split_whitespace().next().unwrap().into(), ..f };
        install(&root, &[f], false, &mut |_| {}).unwrap();
        assert_eq!(fs::read(host(&root, "/usr/local/bin/bindle")).unwrap(), b"new bindle");
    }
}
