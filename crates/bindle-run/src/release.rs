//! Pinned GitHub release assets, fetched as the user. Each download must
//! match the sha256 pinned in the catalog before anything uses it.
//! Packages are returned for the root helper's `pacman -U` (which copies them
//! somewhere root-owned first); AppImages go to ~/.local/bin with a desktop
//! entry that records which asset is installed.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use bindle_core::catalog::{AssetKind, ReleaseAsset};

use crate::user::{file_sha256, run};

/// Download and check every asset; install the AppImages. Returns the
/// package files the root helper should install.
pub fn install(assets: &[ReleaseAsset], fake: bool, note: &mut dyn FnMut(&str)) -> Result<Vec<String>> {
    let home = PathBuf::from(env::var_os("HOME").context("HOME is not set")?);
    let cache = env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".cache"));
    let mut packages = Vec::new();
    for a in assets {
        if fake {
            note(&format!("would download {} and check sha256 {}", a.url(), a.sha256));
            continue;
        }
        let file = fetch(a, &cache.join("bindle/releases").join(&a.tag))?;
        match a.kind() {
            Some(AssetKind::Package) => packages.push(file.to_string_lossy().into_owned()),
            Some(AssetKind::AppImage) => note(&install_appimage(a, &file, &home)?),
            None => bail!("don't know how to install {}", a.asset),
        }
    }
    Ok(packages)
}

/// Download to `dir/<asset>` unless a copy with the right checksum is there.
fn fetch(a: &ReleaseAsset, dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let file = dir.join(&a.asset);
    if file.is_file() && file_sha256(&file)? == a.sha256 {
        return Ok(file);
    }
    let part = dir.join(format!(".{}.part", a.asset));
    let _ = fs::remove_file(&part);
    let argv = ["curl", "-fL", "--retry", "3", "--proto", "=https", "--tlsv1.2", "-o"];
    let mut argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    argv.extend([part.to_string_lossy().into_owned(), a.url()]);
    run(&argv, false, &mut |_| {}).with_context(|| format!("download {}", a.url()))?;
    let got = file_sha256(&part)?;
    if got != a.sha256 {
        let _ = fs::remove_file(&part);
        bail!("{} doesn't match its pinned checksum (got {got}); not installing it", a.asset);
    }
    fs::rename(&part, &file)?;
    Ok(file)
}

/// Returns the line to report.
fn install_appimage(a: &ReleaseAsset, file: &Path, home: &Path) -> Result<String> {
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin)?;
    let target = bin.join(&a.name);
    let tmp = bin.join(format!(".{}.bindle-new", a.name));
    fs::copy(file, &tmp).with_context(|| format!("copy to {}", tmp.display()))?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    fs::rename(&tmp, &target)?;
    let entry = home.join(a.desktop_entry().trim_start_matches("~/"));
    fs::create_dir_all(entry.parent().expect("has a parent"))?;
    fs::write(&entry, desktop_entry(a, &target))?;
    Ok(format!("installed {} ({})", target.display(), a.tag))
}

fn desktop_entry(a: &ReleaseAsset, target: &Path) -> String {
    let title = if a.title.is_empty() { &a.name } else { &a.title };
    format!(
        "[Desktop Entry]\nType=Application\nName={title}\nExec=\"{}\" %U\nTerminal=false\nCategories=Utility;\n\
         # Managed by Bindle\nX-Bindle-Asset={}\n",
        target.display(),
        a.asset
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset() -> ReleaseAsset {
        ReleaseAsset {
            repo: "o/r".into(),
            tag: "v1".into(),
            asset: "r-1-x86_64.AppImage".into(),
            sha256: "0".repeat(64),
            name: "r".into(),
            title: "R app".into(),
        }
    }

    #[test]
    fn desktop_entry_records_the_asset() {
        let e = desktop_entry(&asset(), Path::new("/home/u/.local/bin/r"));
        assert!(e.contains("Name=R app") && e.contains("Exec=\"/home/u/.local/bin/r\" %U"));
        assert!(e.contains("X-Bindle-Asset=r-1-x86_64.AppImage"));
    }

    #[test]
    fn appimages_install_with_a_desktop_entry() {
        let home = crate::sys::test_dir("release-home");
        let file = home.join("download.AppImage");
        fs::write(&file, "#!/bin/sh\n").unwrap();
        install_appimage(&asset(), &file, &home).unwrap();
        let bin = home.join(".local/bin/r");
        assert_eq!(fs::metadata(&bin).unwrap().permissions().mode() & 0o777, 0o755);
        let entry = fs::read_to_string(home.join(".local/share/applications/bindle-r.desktop")).unwrap();
        assert!(entry.contains("X-Bindle-Asset=r-1-x86_64.AppImage"));
    }
}
