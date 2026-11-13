//! Bindle updating itself. The latest release of asafelobotomy/bindle is
//! compared with the running version; a newer one is downloaded and
//! verified the way `install.sh` verifies it: `SHA256SUMS` must carry a
//! valid detached signature (`SHA256SUMS.sig`, checked with `gpgv`) by the
//! release key's fingerprint, and each file must match its line there. The
//! key is `packaging/bindle-release.asc`, built in.
//!
//! What gets replaced depends on where this Bindle runs from: the install
//! `bindle first-login install` made (`/usr/local/bin/bindle`, and
//! `bindle-gui` beside it), through the root helper; an AppImage the user
//! owns, in place. Anything else (a build tree, a distribution package) is
//! only reported.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::system::vercmp;

pub const REPO: &str = "asafelobotomy/bindle";
/// The release key's fingerprint (as in install.sh and SECURITY.md).
pub const SIGNING_FPR: &str = "75C40FA127FBF8D4FBF9E710FAC40EFFE02D69FC";
const KEY: &str = include_str!("../../../packaging/bindle-release.asc");
/// Set in the environment of the Bindle that just replaced itself.
pub const UPDATED_ENV: &str = "BINDLE_UPGRADED";
const INSTALLED: &str = "/usr/local/bin/bindle";
const INSTALLED_GUI: &str = "/usr/local/bin/bindle-gui";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Location {
    /// `/usr/local/bin/bindle` (with `gui`: `bindle-gui` beside it), root's.
    Installed { gui: bool },
    /// An AppImage at this path, in a directory the user can write.
    AppImage(String),
    /// Somewhere Bindle can't update (a build tree, a package).
    Elsewhere(String),
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SelfState {
    pub running: String,
    /// The latest release's tag (`v0.1.9`), or why it couldn't be read;
    /// None when not checked.
    pub latest: Option<Result<String, String>>,
    pub location: Option<Location>,
    /// This run is the Bindle that just replaced the old one.
    pub just_updated: bool,
}

/// Where the running `bindle` (`exe`) lives; `appimage`: `$APPIMAGE`.
pub fn location(exe: &Path, appimage: Option<&Path>, writable: &dyn Fn(&Path) -> bool) -> Location {
    if let Some(a) = appimage {
        return match a.parent() {
            Some(dir) if writable(dir) => Location::AppImage(a.display().to_string()),
            _ => Location::Elsewhere(a.display().to_string()),
        };
    }
    if exe == Path::new(INSTALLED) {
        Location::Installed { gui: Path::new(INSTALLED_GUI).is_file() }
    } else {
        Location::Elsewhere(exe.display().to_string())
    }
}

/// `tag` (`v0.1.9`) is newer than `running` (`0.1.8`).
pub fn newer(running: &str, tag: &str) -> bool {
    vercmp(tag.trim_start_matches('v'), running) == Ordering::Greater
}

/// Release files to fetch and where each goes.
pub fn files(loc: &Location) -> Vec<(&'static str, String)> {
    match loc {
        Location::Installed { gui } => {
            let mut v = vec![("bindle-x86_64", INSTALLED.to_string())];
            if *gui {
                v.push(("bindle-gui-x86_64", INSTALLED_GUI.to_string()));
            }
            v
        }
        Location::AppImage(path) => vec![("Bindle-x86_64.AppImage", path.clone())],
        Location::Elsewhere(_) => vec![],
    }
}

/// The checksum `SHA256SUMS` lists for `file` (`sum  name` or `sum *name`).
pub fn sum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (sum, name) = l.split_once(char::is_whitespace)?;
        let name = name.trim_start().trim_start_matches('*');
        (name == file && sum.len() == 64 && sum.chars().all(|c| c.is_ascii_hexdigit())).then(|| sum.to_lowercase())
    })
}

/// gpgv's `--status-fd` output shows a valid signature by exactly `fpr`.
pub fn valid_signature(status: &str, fpr: &str) -> bool {
    status.lines().any(|l| l.starts_with(&format!("[GNUPG:] VALIDSIG {fpr} ")))
}

/// The latest release's tag, from where github.com/<repo>/releases/latest
/// redirects (no API, so no rate limit).
pub fn latest_tag() -> Result<String, String> {
    let url = format!("https://github.com/{REPO}/releases/latest");
    let out = Command::new("curl")
        .args(["-fsSL", "--max-time", "20", "--proto", "=https", "-o", "/dev/null", "-w", "%{url_effective}", &url])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("can't run curl: {e}"))?;
    let effective = String::from_utf8_lossy(&out.stdout).to_string();
    match effective.rsplit_once("/tag/") {
        Some((_, tag)) if out.status.success() && !tag.is_empty() => Ok(tag.trim().to_string()),
        _ => Err("couldn't read the latest release".into()),
    }
}

fn curl(url: &str, to: &Path) -> Result<()> {
    let ok = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "--proto", "=https", "--tlsv1.2", "-o"])
        .arg(to)
        .arg(url)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        bail!("couldn't download {url}");
    }
    Ok(())
}

fn sha256(path: &Path) -> Result<String> {
    let out = Command::new("sha256sum").arg("--").arg(path).output().context("run sha256sum")?;
    Ok(String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or_default().to_string())
}

/// Download `tag`'s files for `loc` into `dir`, verified; returns (local
/// file, destination, sha256) for each.
pub fn download(tag: &str, loc: &Location, dir: &Path) -> Result<Vec<(PathBuf, String, String)>> {
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir)?;
    let base = format!("https://github.com/{REPO}/releases/download/{tag}");
    curl(&format!("{base}/SHA256SUMS"), &dir.join("SHA256SUMS"))?;
    curl(&format!("{base}/SHA256SUMS.sig"), &dir.join("SHA256SUMS.sig"))
        .context("this release has no signature (SHA256SUMS.sig); not updating")?;
    let gnupg = dir.join("gnupg");
    fs::create_dir_all(&gnupg)?;
    fs::set_permissions(&gnupg, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    let keyring = dir.join("release.gpg");
    let mut dearmor = Command::new("gpg")
        .args(["--batch", "--quiet", "--dearmor", "--output"])
        .arg(&keyring)
        .env("GNUPGHOME", &gnupg)
        .stdin(Stdio::piped())
        .spawn()
        .context("gpg is needed to check the release's signature")?;
    std::io::Write::write_all(&mut dearmor.stdin.take().context("gpg stdin")?, KEY.as_bytes())?;
    dearmor.wait()?;
    let status = Command::new("gpgv")
        .args(["--status-fd", "1", "--keyring"])
        .arg(&keyring)
        .arg(dir.join("SHA256SUMS.sig"))
        .arg(dir.join("SHA256SUMS"))
        .env("GNUPGHOME", &gnupg)
        .stderr(Stdio::null())
        .output()
        .context("gpgv is needed to check the release's signature")?;
    if !valid_signature(&String::from_utf8_lossy(&status.stdout), SIGNING_FPR) {
        bail!("SHA256SUMS isn't signed by Bindle's release key; not updating");
    }
    let sums = fs::read_to_string(dir.join("SHA256SUMS"))?;
    let mut out = vec![];
    for (asset, dest) in files(loc) {
        let want = sum_for(&sums, asset).with_context(|| format!("{asset} isn't in the signed SHA256SUMS"))?;
        let local = dir.join(asset);
        curl(&format!("{base}/{asset}"), &local)?;
        if sha256(&local)? != want {
            bail!("{asset} doesn't match the signed SHA256SUMS; not updating");
        }
        out.push((local, dest, want));
    }
    Ok(out)
}

/// Replace the user's AppImage at `dest` with `file`, atomically.
pub fn replace_appimage(file: &Path, dest: &Path) -> Result<()> {
    let tmp = dest.with_extension("bindle-new");
    fs::copy(file, &tmp).with_context(|| format!("copy to {}", tmp.display()))?;
    fs::set_permissions(&tmp, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
    fs::rename(&tmp, dest).with_context(|| format!("replace {}", dest.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_tags() {
        assert!(newer("0.1.8", "v0.1.9") && newer("0.1.8", "v0.2.0"));
        assert!(!newer("0.1.8", "v0.1.8") && !newer("0.1.9", "v0.1.8"));
    }

    #[test]
    fn where_bindle_runs_decides_what_is_replaced() {
        let yes = |_: &Path| true;
        let no = |_: &Path| false;
        assert!(matches!(location(Path::new(INSTALLED), None, &no), Location::Installed { .. }));
        let app = Path::new("/home/u/Apps/Bindle.AppImage");
        assert_eq!(
            location(Path::new("/tmp/.mount/usr/bin/bindle"), Some(app), &yes),
            Location::AppImage(app.display().to_string())
        );
        assert!(matches!(location(Path::new("/x"), Some(app), &no), Location::Elsewhere(_)));
        assert!(matches!(
            location(Path::new("/home/u/bindle/target/debug/bindle"), None, &yes),
            Location::Elsewhere(_)
        ));
        assert_eq!(files(&Location::Installed { gui: true }).len(), 2);
        assert_eq!(files(&Location::AppImage("/a".into()))[0], ("Bindle-x86_64.AppImage", "/a".to_string()));
        assert!(files(&Location::Elsewhere("/x".into())).is_empty());
    }

    #[test]
    fn checksums_and_signatures() {
        let sum = "a".repeat(64);
        let sums = format!("{sum}  bindle-x86_64\n{}  *Bindle-x86_64.AppImage\n", "B".repeat(64));
        assert_eq!(sum_for(&sums, "bindle-x86_64"), Some(sum));
        assert_eq!(sum_for(&sums, "Bindle-x86_64.AppImage"), Some("b".repeat(64)));
        assert_eq!(sum_for(&sums, "bindle"), None);
        let status = format!("[GNUPG:] GOODSIG X\n[GNUPG:] VALIDSIG {SIGNING_FPR} 2026-11-11 1\n");
        assert!(valid_signature(&status, SIGNING_FPR));
        assert!(!valid_signature("[GNUPG:] VALIDSIG 0000 2026", SIGNING_FPR));
        assert!(KEY.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
    }
}
