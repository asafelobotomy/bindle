//! Open Bindle on each user's first login after install.
//!
//! `bindle first-login install` (root, e.g. from archinstall's custom commands
//! or with `--root /mnt` from the live ISO) installs the binary system-wide plus
//! an XDG autostart entry (desktops), a systemd user service that waits for any
//! graphical session (window managers without XDG autostart: Sway, niri, i3…),
//! and a profile.d hook (console logins). Each user gets the wizard once; a
//! per-user marker records that, and a short-lived lock stops double launches.

use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::apply::sys::{create_dirs, host, is_root, shell_words, write_atomic};

pub const BIN: &str = "/usr/local/bin/bindle";
pub const AUTOSTART: &str = "/etc/xdg/autostart/bindle-first-login.desktop";
pub const PROFILE_HOOK: &str = "/etc/profile.d/bindle-first-login.sh";
pub const LAUNCHER: &str = "/usr/local/share/applications/bindle.desktop";
pub const ICON: &str = "/usr/local/share/icons/hicolor/scalable/apps/bindle.svg";
pub const USER_UNIT: &str = "/etc/systemd/user/bindle-first-login.service";
pub const USER_UNIT_WANTS: &str = "/etc/systemd/user/default.target.wants/bindle-first-login.service";
const SVG: &str = include_str!("../../../branding/bindle.svg");

const USER_UNIT_FILE: &str = "[Unit]
Description=Open Bindle on the first graphical login
# Never for greeters (gdm, sddm, lightdm…) or other system accounts.
ConditionUser=!@system
# Bindle always writes this marker (also when XDG_STATE_HOME points elsewhere).
ConditionPathExists=!%h/.local/state/bindle/first-login-done

[Service]
ExecStart=/usr/local/bin/bindle first-login watch
# The watcher exits once the terminal is open; don't take the terminal with it.
KillMode=process

[Install]
WantedBy=default.target
";

const AUTOSTART_ENTRY: &str = "[Desktop Entry]
Type=Application
Name=Bindle
Comment=Finish setting up your new system
Exec=/usr/local/bin/bindle first-login run
Icon=bindle
Terminal=false
NoDisplay=true
X-GNOME-Autostart-Delay=3
";

const LAUNCHER_ENTRY: &str = "[Desktop Entry]
Type=Application
Name=Bindle
Comment=Set up and optimise this system
Exec=/usr/local/bin/bindle
Icon=bindle
Terminal=true
Categories=System;Settings;
";

const HOOK: &str = r#"# Managed by Bindle: open the setup wizard once, on a user's first console login.
# Only interactive shells: display managers (greetd, ly…) start sessions through
# `sh -l -c`, which also reads /etc/profile on the VT before the desktop starts.
case $- in *i*) ;; *) return 0 2>/dev/null || exit 0 ;; esac
# Regular users only (not root or system accounts), in a `user` session.
_bindle_uid_min=$(awk '$1 == "UID_MIN" { print $2 }' /etc/login.defs 2>/dev/null)
if [ -t 0 ] && [ -t 1 ] && [ -z "${DISPLAY}${WAYLAND_DISPLAY}${SSH_CONNECTION}" ] \
    && [ "$(id -u)" -ge "${_bindle_uid_min:-1000}" ] 2>/dev/null \
    && [ "${XDG_SESSION_CLASS:-user}" = user ] && [ -x /usr/local/bin/bindle ] \
    && [ ! -e "${XDG_STATE_HOME:-$HOME/.local/state}/bindle/first-login-done" ] \
    && [ ! -e "$HOME/.local/state/bindle/first-login-done" ]; then
    /usr/local/bin/bindle first-login run
fi
unset _bindle_uid_min
"#;

fn require_root(root: &Path) -> Result<()> {
    if root == Path::new("/") && !is_root() {
        bail!("needs root: sudo bindle first-login …");
    }
    Ok(())
}

pub fn install(root: &Path, tty_hook: bool) -> Result<Vec<String>> {
    require_root(root)?;
    let mut done = Vec::new();
    let exe = env::current_exe().context("locate own executable")?;
    let target = host(root, BIN);
    let same = fs::canonicalize(&exe).ok() == fs::canonicalize(&target).ok();
    if !same {
        let data = fs::read(&exe).context("read own executable")?;
        write_atomic(&target, &data, 0o755)?;
        done.push(format!("installed {BIN}"));
    }
    let mut files =
        vec![(AUTOSTART, AUTOSTART_ENTRY), (LAUNCHER, LAUNCHER_ENTRY), (ICON, SVG), (USER_UNIT, USER_UNIT_FILE)];
    if tty_hook {
        files.push((PROFILE_HOOK, HOOK));
    }
    for (path, content) in files {
        write_atomic(&host(root, path), content.as_bytes(), 0o644)?;
        done.push(format!("wrote {path}"));
    }
    // What `systemctl --global enable` does, without needing systemd running (chroot).
    let wants = host(root, USER_UNIT_WANTS);
    create_dirs(wants.parent().context("wants dir")?)?;
    let _ = fs::remove_file(&wants);
    symlink("../bindle-first-login.service", &wants).context("enable user unit")?;
    done.push(format!("enabled {USER_UNIT} for all users"));
    Ok(done)
}

pub fn remove(root: &Path, purge: bool) -> Result<Vec<String>> {
    require_root(root)?;
    let mut paths = vec![AUTOSTART, PROFILE_HOOK, USER_UNIT_WANTS, USER_UNIT];
    if purge {
        paths.extend([LAUNCHER, ICON, BIN]);
    }
    let mut done = Vec::new();
    for p in paths {
        let h = host(root, p);
        if h.symlink_metadata().is_ok() {
            fs::remove_file(&h).with_context(|| format!("remove {p}"))?;
            done.push(format!("removed {p}"));
        }
    }
    Ok(done)
}

pub fn status(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = [BIN, AUTOSTART, USER_UNIT, USER_UNIT_WANTS, PROFILE_HOOK, LAUNCHER, ICON]
        .iter()
        .map(|p| format!("{} {p}", if host(root, p).exists() { "✓" } else { "·" }))
        .collect();
    if marker_path().is_some() {
        let state = if is_done() {
            "done — won't open again"
        } else if is_seen() {
            "not finished — reminder notification at next login"
        } else {
            "pending — opens at next login"
        };
        out.push(format!("this user: {state}"));
    }
    out
}

/// Where the "done" marker lives: `$XDG_STATE_HOME` and the default
/// `~/.local/state`. Both are written, so the systemd unit (which can only
/// check `%h/.local/state`) and shells with a different `XDG_STATE_HOME`
/// agree on whether setup is finished.
fn state_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = env::var_os("XDG_STATE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .into_iter()
        .chain(env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map(|s| s.join("bindle"))
        .collect();
    dirs.dedup();
    dirs
}

pub fn marker_path() -> Option<PathBuf> {
    state_dirs().into_iter().next().map(|d| d.join("first-login-done"))
}

fn exists(name: &str) -> bool {
    state_dirs().iter().any(|d| d.join(name).exists())
}

fn mark(name: &str) -> Result<()> {
    let dirs = state_dirs();
    if dirs.is_empty() {
        bail!("no home directory");
    }
    for dir in dirs {
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(name), "")?;
    }
    Ok(())
}

/// The wizard has opened at a login before; later logins get a reminder.
pub fn is_seen() -> bool {
    exists("first-login-seen")
}

pub fn mark_seen() -> Result<()> {
    mark("first-login-seen")
}

pub fn is_done() -> bool {
    exists("first-login-done")
}

pub fn mark_done() -> Result<()> {
    mark("first-login-done")
}

/// JSON to merge into an archinstall configuration file. With `sha256`, the
/// download is verified before it is made executable and run.
pub fn archinstall_snippet(url: &str, sha256: Option<&str>) -> Result<String> {
    let mut cmd = format!("curl -fsSL -o {BIN} {}", shell_words(&[url.to_string()]));
    if let Some(sum) = sha256 {
        if sum.len() != 64 || !sum.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("--sha256 needs a 64-character hex SHA-256 digest");
        }
        cmd.push_str(&format!(" && echo '{}  {BIN}' | sha256sum -c --quiet -", sum.to_ascii_lowercase()));
    }
    cmd.push_str(&format!(" && chmod 755 {BIN} && {BIN} first-login install"));
    Ok(serde_json::to_string_pretty(&serde_json::json!({ "custom_commands": [cmd] }))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_and_remove_into_a_root() {
        let root = crate::apply::sys::test_dir("firstlogin");
        let done = install(&root, true).unwrap();
        assert!(done.iter().any(|d| d.contains(BIN)));
        assert!(host(&root, AUTOSTART).exists() && host(&root, PROFILE_HOOK).exists());
        let entry = fs::read_to_string(host(&root, AUTOSTART)).unwrap();
        assert!(entry.contains("Exec=/usr/local/bin/bindle first-login run"));
        assert_eq!(
            fs::read_link(host(&root, USER_UNIT_WANTS)).unwrap(),
            PathBuf::from("../bindle-first-login.service")
        );
        let unit = fs::read_to_string(host(&root, USER_UNIT)).unwrap();
        assert!(unit.contains("ConditionUser=!@system"));
        assert!(fs::read_to_string(host(&root, PROFILE_HOOK)).unwrap().contains("UID_MIN"));
        remove(&root, false).unwrap();
        assert!(!host(&root, AUTOSTART).exists() && host(&root, BIN).exists());
        assert!(host(&root, USER_UNIT_WANTS).symlink_metadata().is_err());
        remove(&root, true).unwrap();
        assert!(!host(&root, BIN).exists());
    }

    #[test]
    fn snippet_quotes_the_url_and_checks_the_digest() {
        let plain = archinstall_snippet("https://example.invalid/bindle", None).unwrap();
        assert!(plain.contains("first-login install") && !plain.contains("sha256sum"));
        let evil = archinstall_snippet("https://x/'; rm -rf / #", None).unwrap();
        let cmd: serde_json::Value = serde_json::from_str(&evil).unwrap();
        assert!(cmd["custom_commands"][0].as_str().unwrap().contains(r"'https://x/'\''; rm -rf / #'"));
        let sum = "A".repeat(64);
        let checked = archinstall_snippet("https://x/b", Some(&sum)).unwrap();
        assert!(checked.contains(&format!("{}  {BIN}' | sha256sum -c", "a".repeat(64))));
        assert!(archinstall_snippet("https://x/b", Some("abc")).is_err());
    }
}
