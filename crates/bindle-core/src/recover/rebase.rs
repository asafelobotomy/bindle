//! Rebase: the system as installed, again. The packages the installer put
//! there and the ones Bindle set up are reinstalled (and put back if they
//! were removed); their config files that were changed go back to the
//! package's version, the changed copy kept; Bindle's setup is applied
//! again. The user's home, packages they added themselves and the files
//! that make this machine itself (accounts, disks, names, boot) stay.

use std::collections::BTreeSet;

/// Config files a rebase never resets: they hold this machine's accounts,
/// disks, name, language and boot setup, written at install time.
pub const KEEP: &[&str] = &[
    "/etc/passwd",
    "/etc/shadow",
    "/etc/group",
    "/etc/gshadow",
    "/etc/subuid",
    "/etc/subgid",
    "/etc/fstab",
    "/etc/crypttab",
    "/etc/hostname",
    "/etc/hosts",
    "/etc/locale.gen",
    "/etc/locale.conf",
    "/etc/vconsole.conf",
    "/etc/localtime",
    "/etc/mkinitcpio.conf",
    "/etc/mkinitcpio.d/",
    "/etc/default/grub",
    "/etc/kernel/cmdline",
    "/etc/sudoers",
    "/etc/sudoers.d/",
    "/etc/pacman.d/mirrorlist",
    "/etc/machine-id",
    "/etc/ssh/",
    "/etc/NetworkManager/system-connections/",
    "/etc/iwd/",
    "/etc/X11/xorg.conf.d/00-keyboard.conf",
];

/// Whether a rebase leaves `path` as it is.
pub fn kept(path: &str) -> bool {
    KEEP.iter().any(|k| if k.ends_with('/') { path.starts_with(k) } else { path == *k })
}

/// A package's config file whose content differs from the package's.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Changed {
    pub package: String,
    pub path: String,
}

/// From `pacman -Qkk`: changed config files (backup files whose size or
/// checksum differs; a new modification time alone isn't a change), of the
/// packages in `stock`, outside `KEEP`.
pub fn changed_configs(qkk: &str, stock: &BTreeSet<String>) -> Vec<Changed> {
    let mut out = BTreeSet::new();
    for line in qkk.lines() {
        let Some(rest) = line.strip_prefix("backup file: ") else { continue };
        let Some((package, rest)) = rest.split_once(": ") else { continue };
        let Some((path, why)) = rest.rsplit_once(" (") else { continue };
        let changed = why.starts_with("SHA256 checksum mismatch") || why.starts_with("Size mismatch");
        if changed && stock.contains(package) && path.starts_with("/etc/") && !kept(path) {
            out.insert(Changed { package: package.to_string(), path: path.to_string() });
        }
    }
    out.into_iter().collect()
}

/// What a rebase reinstalls: the stock packages still installed, and the
/// ones that were removed (put back). Packages no repository has any more
/// (`known` says) are left out.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Packages {
    pub reinstall: Vec<String>,
    pub restore: Vec<String>,
}

pub fn packages(stock: &BTreeSet<String>, installed: &BTreeSet<String>, known: &dyn Fn(&str) -> bool) -> Packages {
    let mut p = Packages::default();
    for name in stock.iter().filter(|n| known(n)) {
        if installed.contains(name) {
            p.reinstall.push(name.clone());
        } else {
            p.restore.push(name.clone());
        }
    }
    p
}

/// The stock set: every package named in the installer's pacman runs (from
/// pacman.log) and every package Bindle installed (its ledger).
pub fn stock(pacman_log: &str, bindle: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    let (runs, _) = crate::install::pacman_log::transactions(pacman_log);
    runs.into_iter().flat_map(|t| t.packages).chain(bindle).collect()
}

/// Shell-quote one word.
fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The script (for `sh -ec`) that resets `files` to their package's
/// version, keeping each changed copy under `backup` (same path inside it).
/// The package file comes from pacman's cache (the reinstall just put the
/// installed version there).
pub fn reset_script(files: &[Changed], backup: &str) -> String {
    let mut s = format!(
        r#"backup={}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
reset() {{
    pkg=$1; path=$2
    ver=$(pacman -Q "$pkg" | cut -d' ' -f2)
    file=
    for f in /var/cache/pacman/pkg/"$pkg-$ver"-*.pkg.tar.*; do
        case "$f" in *.sig) ;; *) [ -f "$f" ] && file=$f ;; esac
    done
    if [ -z "$file" ]; then echo "kept $path: $pkg $ver isn't in pacman's cache"; return 0; fi
    rel=${{path#/}}
    bsdtar -xpf "$file" -C "$work" "$rel"
    mkdir -p "$backup$(dirname "$path")"
    cp -a "$path" "$backup$path"
    cp -a "$work/$rel" "$path"
    echo "reset $path ($pkg)"
}}
"#,
        q(backup)
    );
    for f in files {
        s.push_str(&format!("reset {} {}\n", q(&f.package), q(&f.path)));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    const QKK: &str = "\
backup file: pacman: /etc/pacman.conf (Modification time mismatch)
backup file: pacman: /etc/pacman.conf (Size mismatch)
backup file: pacman: /etc/pacman.conf (SHA256 checksum mismatch)
backup file: filesystem: /etc/fstab (SHA256 checksum mismatch)
backup file: mpd: /etc/mpd.conf (SHA256 checksum mismatch)
backup file: systemd: /etc/systemd/logind.conf (Modification time mismatch)
warning: filesystem: /etc/shadow (failed to calculate SHA256 checksum)
pacman: 426 total files, 0 altered files
";

    #[test]
    fn only_changed_stock_configs_outside_keep() {
        let stock = set(&["pacman", "filesystem", "systemd"]);
        let c = changed_configs(QKK, &stock);
        assert_eq!(c, vec![Changed { package: "pacman".into(), path: "/etc/pacman.conf".into() }]);
    }

    #[test]
    fn keep_covers_the_machines_own_files() {
        for p in ["/etc/fstab", "/etc/sudoers.d/10-wheel", "/etc/ssh/sshd_config", "/etc/hostname"] {
            assert!(kept(p), "{p}");
        }
        assert!(!kept("/etc/pacman.conf") && !kept("/etc/sshd"));
    }

    #[test]
    fn reinstalls_what_is_there_and_restores_what_was_removed() {
        let stock = set(&["base", "nano", "gone-from-repos"]);
        let installed = set(&["base", "firefox"]);
        let p = packages(&stock, &installed, &|n| n != "gone-from-repos");
        assert_eq!(p, Packages { reinstall: vec!["base".into()], restore: vec!["nano".into()] });
    }

    #[test]
    fn stock_is_the_installers_runs_plus_bindles() {
        let log = "[2026-11-01T10:00:00+0000] [PACMAN] Running 'pacman -r /mnt -S --noconfirm base linux'\n\
                   [2026-11-02T10:00:00+0000] [PACMAN] Running 'pacman -S firefox'\n";
        assert_eq!(stock(log, ["cups".to_string()]), set(&["base", "linux", "cups"]));
    }

    #[test]
    fn the_reset_script_is_valid_shell() {
        let files = [Changed { package: "pacman".into(), path: "/etc/pacman.conf".into() }];
        let s = reset_script(&files, "/var/lib/bindle/rebase/x");
        assert!(s.ends_with("reset 'pacman' '/etc/pacman.conf'\n"));
        assert!(std::process::Command::new("sh").args(["-n", "-c", &s]).status().unwrap().success());
    }
}
