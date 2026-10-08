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
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};

use crate::apply::sys::{host, is_root, write_atomic};

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
if [ -t 0 ] && [ -t 1 ] && [ -z "${DISPLAY}${WAYLAND_DISPLAY}${SSH_CONNECTION}" ] \
    && [ "$(id -u)" -ne 0 ] && [ -x /usr/local/bin/bindle ] \
    && [ ! -e "${XDG_STATE_HOME:-$HOME/.local/state}/bindle/first-login-done" ]; then
    /usr/local/bin/bindle first-login run
fi
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
    fs::create_dir_all(wants.parent().context("wants dir")?)?;
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
    if let Some(m) = marker_path() {
        let state = if m.exists() {
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

pub fn marker_path() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map(|s| s.join("bindle/first-login-done"))
}

fn seen_path() -> Option<PathBuf> {
    marker_path().map(|m| m.with_file_name("first-login-seen"))
}

/// The wizard has opened at a login before; later logins get a reminder.
pub fn is_seen() -> bool {
    seen_path().is_some_and(|m| m.exists())
}

pub fn mark_seen() -> Result<()> {
    let m = seen_path().context("no home directory")?;
    if let Some(dir) = m.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&m, "")?;
    Ok(())
}

pub fn is_done() -> bool {
    marker_path().is_some_and(|m| m.exists())
}

pub fn mark_done() -> Result<()> {
    let m = marker_path().context("no home directory")?;
    if let Some(dir) = m.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&m, "")?;
    Ok(())
}

fn runtime_dir() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR").map_or_else(env::temp_dir, PathBuf::from)
}

fn lock_path() -> PathBuf {
    runtime_dir().join("bindle-first-login.lock")
}

/// Claim the right to open the wizard now. Autostart and the user service can
/// both fire in one session; the lock expires so a crash can't block forever.
pub fn claim_launch() -> bool {
    let lock = lock_path();
    let fresh = fs::metadata(&lock)
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() < Duration::from_secs(600));
    !fresh && fs::write(&lock, std::process::id().to_string()).is_ok()
}

pub fn release_launch() {
    let _ = fs::remove_file(lock_path());
}

/// The graphical session's display, from its sockets: Wayland first (an X11
/// socket in a Wayland session is just Xwayland), then X11.
pub fn find_display(runtime: &Path, x11_dir: &Path) -> Option<(&'static str, String)> {
    let names = |dir: &Path| -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    if let Some(w) = names(runtime).into_iter().find(|n| n.starts_with("wayland-") && !n.ends_with(".lock")) {
        return Some(("WAYLAND_DISPLAY", w));
    }
    names(x11_dir).into_iter().find_map(|n| n.strip_prefix('X').map(|d| ("DISPLAY", format!(":{d}"))))
}

/// Type of this user's active session with a display, per logind (`x11`,
/// `wayland`, or `tty` for sessions greetd starts on a VT).
fn active_session_type() -> Option<String> {
    let user = env::var("USER").ok()?;
    let out = |args: &[&str]| -> Option<String> {
        let o = std::process::Command::new("loginctl").args(args).output().ok()?;
        let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
        (o.status.success() && !v.is_empty()).then_some(v)
    };
    let session = out(&["show-user", &user, "-p", "Display", "--value"])?;
    let active = out(&["show-session", &session, "-p", "State", "--value"])? == "active";
    let kind = out(&["show-session", &session, "-p", "Type", "--value"])?;
    active.then_some(kind)
}

/// Wait for this user's graphical session (however long: the user manager may
/// start at a console or SSH login first), then open the wizard in it.
pub fn watch() -> Result<()> {
    loop {
        if is_done() || is_root() {
            return Ok(());
        }
        // Wayland sockets live in this user's private runtime dir, so any active
        // session will do (greetd sessions report `tty`). X11 sockets are global
        // (the greeter has one too): only trust one in an x11 session.
        if let Some(kind) = active_session_type()
            && let Some((key, value)) = find_display(&runtime_dir(), Path::new("/tmp/.X11-unix"))
            && (key == "WAYLAND_DISPLAY" || kind == "x11")
        {
            // Give the compositor/WM a moment to start mapping windows.
            std::thread::sleep(Duration::from_secs(6));
            let mut cmd = std::process::Command::new(env::current_exe()?);
            cmd.args(["first-login", "run"]).env(key, value).stdin(std::process::Stdio::null());
            if let Some(xauth) =
                env::var_os("HOME").map(|h| PathBuf::from(h).join(".Xauthority")).filter(|p| p.exists())
            {
                cmd.env("XAUTHORITY", xauth);
            }
            cmd.status().context("start the first-login wizard")?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// JSON to merge into an archinstall configuration file.
pub fn archinstall_snippet(url: &str) -> String {
    let cmd = format!("curl -fsSL -o {BIN} '{url}' && chmod 755 {BIN} && {BIN} first-login install");
    serde_json::to_string_pretty(&serde_json::json!({ "custom_commands": [cmd] })).unwrap_or_default()
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
        remove(&root, false).unwrap();
        assert!(!host(&root, AUTOSTART).exists() && host(&root, BIN).exists());
        assert!(host(&root, USER_UNIT_WANTS).symlink_metadata().is_err());
        remove(&root, true).unwrap();
        assert!(!host(&root, BIN).exists());
        assert!(archinstall_snippet("https://example.invalid/bindle").contains("first-login install"));
    }

    #[test]
    fn finds_wayland_before_x11() {
        let root = crate::apply::sys::test_dir("display");
        let (run, x11) = (root.join("run"), root.join("x11"));
        fs::create_dir_all(&run).unwrap();
        fs::create_dir_all(&x11).unwrap();
        assert_eq!(find_display(&run, &x11), None);
        fs::write(x11.join("X0"), "").unwrap();
        assert_eq!(find_display(&run, &x11), Some(("DISPLAY", ":0".into())));
        fs::write(run.join("wayland-1.lock"), "").unwrap();
        fs::write(run.join("wayland-1"), "").unwrap();
        assert_eq!(find_display(&run, &x11), Some(("WAYLAND_DISPLAY", "wayland-1".into())));
    }
}
