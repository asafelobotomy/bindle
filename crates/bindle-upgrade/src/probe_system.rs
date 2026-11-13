//! Reading the system for the safety checks and the closing stages: free
//! space, power, kernels, DKMS, /boot, failed units, rebuilds, uQ, and
//! Bindle's own release. The parsing is in `system` and `selfupdate`.

use std::fs;
use std::path::Path;

use super::{age_days, capture, on_path};
use crate::selfupdate::{self, SelfState, UPDATED_ENV};
use crate::system::{self, Dkms, Kernel, Mount, Pin, Power, Rebuild};

pub(super) fn bindle(exe: &Path) -> SelfState {
    let appimage = std::env::var_os("APPIMAGE").map(std::path::PathBuf::from);
    let writable = |dir: &Path| {
        let probe = dir.join(format!(".bindle-write-check-{}", std::process::id()));
        let ok = fs::write(&probe, b"").is_ok();
        let _ = fs::remove_file(&probe);
        ok
    };
    SelfState {
        running: env!("CARGO_PKG_VERSION").to_string(),
        latest: Some(selfupdate::latest_tag()),
        location: Some(selfupdate::location(exe, appimage.as_deref(), &writable)),
        just_updated: std::env::var_os(UPDATED_ENV).is_some(),
    }
}

pub(super) fn mounts() -> Vec<Mount> {
    let paths: Vec<&str> = ["/", "/boot", "/efi", "/boot/efi"].into_iter().filter(|p| Path::new(p).exists()).collect();
    let mut argv = vec!["df", "--output=target,fstype,avail", "-BM"];
    argv.extend(paths);
    capture(&argv).map(|(_, out, _)| system::df(&out)).unwrap_or_default()
}

pub(super) fn power() -> Option<Power> {
    let dir = fs::read_dir("/sys/class/power_supply").ok()?;
    let read = |p: &Path, f: &str| fs::read_to_string(p.join(f)).ok().map(|s| s.trim().to_string());
    let supplies: Vec<(String, Option<bool>, Option<u8>)> = dir
        .flatten()
        .map(|e| e.path())
        .map(|p| {
            let kind = read(&p, "type").unwrap_or_default();
            let online = read(&p, "online").map(|o| o == "1");
            let capacity = read(&p, "capacity").and_then(|c| c.parse().ok());
            (kind, online, capacity)
        })
        .collect();
    system::power(&supplies)
}

pub(super) fn pin() -> Pin {
    let depends = capture(&["pacman", "-Qi", "uq-hyprland-pin"])
        .map(|(_, out, _)| {
            out.lines()
                .find(|l| l.starts_with("Depends On"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, d)| d.to_string())
        })
        .ok()
        .flatten()
        .unwrap_or_default();
    system::pin(&depends)
}

pub(super) fn kernels() -> Vec<Kernel> {
    let Ok(dir) = fs::read_dir("/usr/lib/modules") else { return vec![] };
    let mut out: Vec<Kernel> = dir
        .flatten()
        .filter_map(|e| {
            let pkgbase = fs::read_to_string(e.path().join("pkgbase")).ok()?;
            Some(Kernel { version: e.file_name().to_string_lossy().into_owned(), pkgbase: pkgbase.trim().to_string() })
        })
        .collect();
    out.sort_by(|a, b| a.version.cmp(&b.version));
    out
}

pub(super) fn dkms() -> Vec<Dkms> {
    if !on_path("dkms") {
        return vec![];
    }
    capture(&["dkms", "status"]).map(|(_, out, _)| system::dkms(&out)).unwrap_or_default()
}

pub(super) fn boot() -> Option<Vec<String>> {
    let dir = fs::read_dir("/boot").ok()?;
    Some(dir.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
}

pub(super) fn failed() -> Vec<String> {
    let mut out = vec![];
    for scope in [None, Some("--user")] {
        let mut argv = vec!["systemctl"];
        argv.extend(scope);
        argv.extend(["--failed", "--plain", "--no-legend", "--no-pager"]);
        if let Ok((_, text, _)) = capture(&argv) {
            out.extend(system::failed_units(&text));
        }
    }
    out
}

pub(super) fn rebuild() -> Option<Vec<Rebuild>> {
    if !on_path("checkrebuild") {
        return None;
    }
    Some(capture(&["checkrebuild"]).map(|(_, out, _)| system::checkrebuild(&out)).unwrap_or_default())
}

pub(super) fn greeter() -> bool {
    fs::read_link("/etc/systemd/system/display-manager.service").is_ok_and(|t| t.to_string_lossy().contains("greetd"))
}

/// The age of fwupd's firmware list in days, refreshed first when it is
/// older than `max_days` and `refresh` allows (only a metadata download,
/// through fwupd's own permissions; a failure leaves the old list).
pub(super) fn firmware(max_days: i64, refresh: bool) -> Option<i64> {
    let age = || {
        fs::read_dir("/var/lib/fwupd/metadata")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|remote| fs::read_dir(remote.path()).ok())
            .flatten()
            .flatten()
            .filter_map(|f| age_days(&f.path()))
            .min()
    };
    if refresh && crate::system::stale(age(), max_days) {
        let _ = capture(&["fwupdmgr", "refresh", "--force"]);
    }
    age()
}
