//! OS release, mounts, packages, services, users and config drop-ins.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

use super::read;

pub fn os_field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).map(|v| v.trim().trim_matches('"').to_string())
}

fn mount_points(mounts: &str) -> impl Iterator<Item = (&str, &str)> {
    mounts.lines().filter_map(|l| {
        let mut it = l.split_whitespace();
        let (_dev, mnt, fs) = (it.next()?, it.next()?, it.next()?);
        Some((mnt, fs))
    })
}

/// Filesystem type of `/`; the last matching mount wins (overmounts).
pub fn root_fs(mounts: &str) -> String {
    mount_points(mounts)
        .filter(|(mnt, _)| *mnt == "/")
        .map(|(_, fs)| fs.to_string())
        .last()
        .unwrap_or_else(|| "unknown".into())
}

/// `/.snapshots` is mounted separately (e.g. archinstall's `@.snapshots`).
pub fn snapshots_mounted(mounts: &str) -> bool {
    mount_points(mounts).any(|(mnt, _)| mnt == "/.snapshots")
}

pub(super) fn list_dir(dir: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// sysctl.d search path, highest precedence first.
const SYSCTL_DIRS: &[&str] = &["/etc/sysctl.d", "/run/sysctl.d", "/usr/lib/sysctl.d"];

/// First effective sysctl.d file (a name in /etc shadows the same name in
/// /run and /usr/lib) that sets `key`, ignoring `own`.
pub fn sysctl_owner(
    key: &str,
    own: &str,
    list: &dyn Fn(&str) -> Vec<String>,
    read: &dyn Fn(&str) -> String,
) -> Option<String> {
    let mut seen = BTreeSet::new();
    for dir in SYSCTL_DIRS {
        for name in list(dir) {
            if !name.ends_with(".conf") || !seen.insert(name.clone()) || name == own {
                continue;
            }
            let path = format!("{dir}/{name}");
            if read(&path)
                .lines()
                .any(|l| l.trim_start().strip_prefix(key).is_some_and(|r| r.trim_start().starts_with('=')))
            {
                return Some(path);
            }
        }
    }
    None
}

pub(super) fn swap_sysctl_owner() -> Option<String> {
    sysctl_owner("vm.swappiness", "99-bindle-zram.conf", &list_dir, &read)
}

/// zram-generator configs in effect: the main file in /etc, /run or
/// /usr/lib, plus any `zram-generator.conf.d/*.conf` drop-ins.
pub fn zram_config_paths(list: &dyn Fn(&str) -> Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for base in ["/etc/systemd", "/run/systemd", "/usr/lib/systemd"] {
        if list(base).iter().any(|n| n == "zram-generator.conf") {
            out.push(format!("{base}/zram-generator.conf"));
        }
        let dropins = format!("{base}/zram-generator.conf.d");
        out.extend(list(&dropins).into_iter().filter(|n| n.ends_with(".conf")).map(|n| format!("{dropins}/{n}")));
    }
    out
}

/// AUR helpers are usually installed as `-bin`/`-git` builds (paru-bin provides paru).
pub fn aur_helper(installed: &BTreeSet<String>) -> Option<String> {
    ["paru", "yay"]
        .into_iter()
        .find(|h| ["", "-bin", "-git"].iter().any(|suffix| installed.contains(&format!("{h}{suffix}"))))
        .map(String::from)
}

pub(super) fn display_manager() -> Option<String> {
    let target = fs::read_link("/etc/systemd/system/display-manager.service").ok()?;
    let name = target.file_name()?.to_string_lossy().into_owned();
    Some(name.trim_end_matches(".service").to_string())
}

/// Units enabled via `[Install]` symlinks in /etc/systemd/system/*.wants.
pub(super) fn enabled_units() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(dirs) = fs::read_dir("/etc/systemd/system") else { return out };
    for dir in dirs.flatten() {
        let name = dir.file_name().to_string_lossy().into_owned();
        if name.ends_with(".wants") || name.ends_with(".requires") {
            if let Ok(units) = fs::read_dir(dir.path()) {
                out.extend(units.flatten().map(|u| u.file_name().to_string_lossy().into_owned()));
            }
        } else if name == "display-manager.service" {
            out.insert(name);
        }
    }
    out
}

fn stdout(cmd: &mut Command) -> Option<String> {
    cmd.output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}

pub(super) fn package_groups() -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in stdout(Command::new("pacman").arg("-Sgg")).unwrap_or_default().lines() {
        if let Some((group, pkg)) = line.split_once(' ') {
            out.entry(group.to_string()).or_default().insert(pkg.to_string());
        }
    }
    out
}

pub(super) fn installed_packages() -> BTreeSet<String> {
    stdout(Command::new("pacman").arg("-Qq")).map(|s| s.lines().map(String::from).collect()).unwrap_or_default()
}

/// Supplementary and primary groups of `user` (`id -Gn`).
pub(super) fn user_groups(user: &str) -> BTreeSet<String> {
    if user.is_empty() {
        return BTreeSet::new();
    }
    stdout(Command::new("id").args(["-Gn", "--", user]))
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default()
}

/// Name of the user this process runs as, from the passwd database. Not
/// `$SUDO_USER`/`$USER`: after `sudo -u dev -i`, SUDO_USER names someone else,
/// and group changes would go to the wrong account.
pub(super) fn current_user() -> String {
    use std::os::unix::fs::MetadataExt;
    let Ok(uid) = std::fs::metadata("/proc/self").map(|m| m.uid()) else {
        return std::env::var("USER").unwrap_or_default();
    };
    stdout(Command::new("getent").args(["passwd", &uid.to_string()]))
        .and_then(|s| passwd_name(&s))
        .unwrap_or_else(|| std::env::var("USER").unwrap_or_default())
}

/// uid and gid this process runs as.
pub(super) fn current_ids() -> (u32, u32) {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").map_or((0, 0), |m| (m.uid(), m.gid()))
}

fn passwd_name(getent: &str) -> Option<String> {
    getent.lines().next()?.split(':').next().filter(|n| !n.is_empty()).map(String::from)
}

/// Home directory from `getent passwd` (not $HOME, which sudo may change).
pub(super) fn user_home(user: &str) -> String {
    if user.is_empty() {
        return String::new();
    }
    stdout(Command::new("getent").args(["passwd", "--", user]))
        .and_then(|s| s.lines().next()?.split(':').nth(5).map(String::from))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_comes_from_the_uid_not_the_environment() {
        assert_eq!(passwd_name("dev:x:1001:1001::/home/dev:/bin/bash\n").as_deref(), Some("dev"));
        assert_eq!(passwd_name(""), None);
        // SAFETY: test-only; no other thread in this test reads SUDO_USER.
        unsafe { std::env::set_var("SUDO_USER", "someone-else") };
        let user = current_user();
        assert!(!user.is_empty() && user != "someone-else");
    }

    #[test]
    fn parses_os_release() {
        let os = "NAME=\"Arch Linux\"\nID=arch\nID_LIKE=\"arch\"\n";
        assert_eq!(os_field(os, "ID").as_deref(), Some("arch"));
        assert_eq!(os_field(os, "ID_LIKE").as_deref(), Some("arch"));
        assert_eq!(os_field(os, "VERSION"), None);
    }

    #[test]
    fn parses_mounts() {
        let mounts = "rootfs / rootfs rw 0 0\n/dev/nvme0n1p2 / btrfs rw 0 0\n/dev/nvme0n1p2 /.snapshots btrfs rw 0 0\n";
        assert_eq!(root_fs(mounts), "btrfs");
        assert!(snapshots_mounted(mounts));
        assert!(!snapshots_mounted("/dev/sda2 / ext4 rw 0 0\n"));
    }

    #[test]
    fn aur_helper_variants() {
        let set = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(aur_helper(&set(&["paru-bin"])).as_deref(), Some("paru"));
        assert_eq!(aur_helper(&set(&["yay-git"])).as_deref(), Some("yay"));
        assert_eq!(aur_helper(&set(&["pacman"])), None);
    }

    #[test]
    fn sysctl_owner_respects_shadowing_and_all_dirs() {
        let list = |d: &str| match d {
            "/etc/sysctl.d" => vec!["99-bindle-zram.conf".into(), "10-local.conf".into()],
            "/usr/lib/sysctl.d" => vec!["10-local.conf".into(), "70-cachyos.conf".into()],
            _ => vec![],
        };
        let read = |p: &str| match p {
            "/usr/lib/sysctl.d/10-local.conf" | "/usr/lib/sysctl.d/70-cachyos.conf" => "vm.swappiness = 100\n".into(),
            "/etc/sysctl.d/99-bindle-zram.conf" => "vm.swappiness=180\n".into(),
            _ => String::new(),
        };
        // /etc/sysctl.d/10-local.conf shadows the /usr/lib one.
        assert_eq!(
            sysctl_owner("vm.swappiness", "99-bindle-zram.conf", &list, &read).as_deref(),
            Some("/usr/lib/sysctl.d/70-cachyos.conf")
        );
        assert_eq!(sysctl_owner("vm.swappiness_x", "", &list, &read), None);
    }

    #[test]
    fn zram_configs_include_vendor_and_dropins() {
        let list = |d: &str| match d {
            "/usr/lib/systemd" => vec!["zram-generator.conf".into(), "system".into()],
            "/etc/systemd/zram-generator.conf.d" => vec!["50-size.conf".into(), "README".into()],
            _ => vec![],
        };
        assert_eq!(
            zram_config_paths(&list),
            ["/etc/systemd/zram-generator.conf.d/50-size.conf", "/usr/lib/systemd/zram-generator.conf"]
        );
        assert!(zram_config_paths(&|_| vec![]).is_empty());
    }
}
