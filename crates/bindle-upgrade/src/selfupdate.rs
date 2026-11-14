//! Bindle updating itself. The latest release of asafelobotomy/bindle is
//! compared with the running version; a newer one is downloaded and
//! verified the way `install.sh` verifies it: `SHA256SUMS` must carry a
//! valid detached signature (`SHA256SUMS.sig`, checked with `gpgv`) by the
//! release key's fingerprint, and each file must match its line there. The
//! key is `packaging/bindle-release.asc`, built in.
//!
//! Downgrades: the release's `VERSION` file (listed in the signed
//! `SHA256SUMS`, so covered by the signature) must hold exactly the tag being
//! installed, and that tag must be newer than the running Bindle. A signed
//! old release served in place of the latest one is refused.
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
/// Every fingerprint a release may be signed by. To rotate the key, a release
/// adds the next one here (and its public key to `bindle-release.asc`)
/// before the current key expires (2028-11-07); see SECURITY.md.
pub const SIGNING_FPRS: &[&str] = &[SIGNING_FPR];
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

/// A release tag as install.sh accepts it:
/// `^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$`. Checked before a tag
/// from the network goes into a URL or a cache path.
pub fn valid_tag(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else { return false };
    let (core, pre) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && pre.is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.'))
}

/// The release's signed `VERSION` file (`v0.1.9` and a newline) names
/// `tag`, and `tag` is newer than `running`: refuses a downgrade or a
/// release passed off as another.
pub fn check_version(version_file: &str, tag: &str, running: &str) -> Result<()> {
    let named = version_file.strip_suffix('\n').unwrap_or(version_file);
    if named != tag {
        bail!("the release's signed VERSION says {named:?}, not {tag}; not updating");
    }
    if !newer(running, tag) {
        bail!("{tag} isn't newer than this Bindle ({running}); not downgrading");
    }
    Ok(())
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

/// gpgv's `--status-fd` output shows a valid signature by exactly one of `fprs`.
pub fn valid_signature(status: &str, fprs: &[&str]) -> bool {
    status.lines().any(|l| fprs.iter().any(|fpr| l.starts_with(&format!("[GNUPG:] VALIDSIG {fpr} "))))
}

/// The latest release's tag, from where github.com/<repo>/releases/latest
/// redirects (no API, so no rate limit).
pub fn latest_tag() -> Result<String, String> {
    let url = format!("https://github.com/{REPO}/releases/latest");
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "20",
            "--proto",
            "=https",
            "--tlsv1.2",
            "-o",
            "/dev/null",
            "-w",
            "%{url_effective}",
            &url,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("can't run curl: {e}"))?;
    let effective = String::from_utf8_lossy(&out.stdout).to_string();
    match effective.rsplit_once("/tag/") {
        Some((_, tag)) if out.status.success() && valid_tag(tag.trim()) => Ok(tag.trim().to_string()),
        Some((_, tag)) if out.status.success() => Err(format!("the latest release's tag isn't a version: {tag:?}")),
        _ => Err("couldn't read the latest release".into()),
    }
}

fn curl(url: &str, to: &Path) -> Result<()> {
    let ok = Command::new("curl")
        // An AppImage on a slow link: generous, but not forever.
        .args(["-fsSL", "--retry", "3", "--proto", "=https", "--tlsv1.2", "--max-time", "900", "-o"])
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

/// Download `tag`'s files for `loc` into `dir`, verified, when `tag` is
/// newer than `running` and the release's signed VERSION names it; returns
/// (local file, destination, sha256) for each.
pub fn download(tag: &str, running: &str, loc: &Location, dir: &Path) -> Result<Vec<(PathBuf, String, String)>> {
    if !valid_tag(tag) {
        bail!("{tag:?} isn't a release tag; not updating");
    }
    if !newer(running, tag) {
        bail!("{tag} isn't newer than this Bindle ({running}); not downgrading");
    }
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
    if !valid_signature(&String::from_utf8_lossy(&status.stdout), SIGNING_FPRS) {
        bail!("SHA256SUMS isn't signed by Bindle's release key; not updating");
    }
    let sums = fs::read_to_string(dir.join("SHA256SUMS"))?;
    let want = sum_for(&sums, "VERSION").context("this release has no signed VERSION file; not updating")?;
    let version = dir.join("VERSION");
    curl(&format!("{base}/VERSION"), &version)?;
    if sha256(&version)? != want {
        bail!("VERSION doesn't match the signed SHA256SUMS; not updating");
    }
    check_version(&fs::read_to_string(&version)?, tag, running)?;
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
    fn tags_from_the_network_are_checked() {
        for ok in ["v0.1.9", "v10.20.30", "v0.2.0-rc.1", "v1.0.0-beta2"] {
            assert!(valid_tag(ok), "{ok}");
        }
        for bad in [
            "",
            "v",
            "0.1.9",
            "v0.1",
            "v0.1.9.1",
            "v0.1.x",
            "v0.1.9-",
            "v0.1.9-a/b",
            "v0.1.9/../..",
            "../v0.1.9",
            "v0.1.9\n",
            "v0.1.9 ",
            "v.1.9",
            "v0..9",
            "v0.1.9-rc_1",
        ] {
            assert!(!valid_tag(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_signed_version_must_name_a_newer_tag() {
        assert!(check_version("v0.1.9\n", "v0.1.9", "0.1.8").is_ok());
        assert!(check_version("v0.1.9", "v0.1.9", "0.1.8").is_ok());
        // An old signed release served as the latest one.
        assert!(check_version("v0.1.7\n", "v0.1.9", "0.1.8").is_err());
        assert!(check_version("v0.1.9\n\n", "v0.1.9", "0.1.8").is_err());
        assert!(check_version(" v0.1.9\n", "v0.1.9", "0.1.8").is_err());
        // Same or older than the running one.
        assert!(check_version("v0.1.8\n", "v0.1.8", "0.1.8").is_err());
        assert!(check_version("v0.1.7\n", "v0.1.7", "0.1.8").is_err());
        let err = check_version("v0.1.7\n", "v0.1.7", "0.1.8").unwrap_err().to_string();
        assert!(err.contains("not downgrading"), "{err}");
    }

    #[test]
    fn download_refuses_bad_or_older_tags_before_touching_anything() {
        let dir = std::env::temp_dir().join(format!("bindle-selfupdate-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("keep"), b"").unwrap();
        let loc = Location::AppImage("/nonexistent/Bindle.AppImage".into());
        assert!(download("../../x", "0.1.8", &loc, &dir).is_err());
        assert!(download("v0.1.7", "0.1.8", &loc, &dir).is_err());
        assert!(download("v0.1.8", "0.1.8", &loc, &dir).is_err());
        // Refused before the old download directory was cleared.
        assert!(dir.join("keep").exists());
        fs::remove_dir_all(&dir).unwrap();
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
        assert!(valid_signature(&status, SIGNING_FPRS));
        assert!(valid_signature(&status, &["0000", SIGNING_FPR]));
        assert!(!valid_signature("[GNUPG:] VALIDSIG 0000 2026", SIGNING_FPRS));
        assert!(!valid_signature(&status, &[]));
        assert!(KEY.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
    }
}
