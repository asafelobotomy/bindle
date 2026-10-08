//! Who and where the first-login wizard may open: regular users in a `user`
//! logind session (never greeters or system accounts), once per session
//! (launch lock), in a graphical session found by `watch`.

use std::env;
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};

use crate::firstlogin::is_done;

/// Lock age after which a crashed launch no longer blocks the next one.
const LOCK_STALE: Duration = Duration::from_secs(600);

/// `UID_MIN` from login.defs; 1000 when unset or unreadable.
pub fn uid_min(login_defs: &str) -> u32 {
    login_defs
        .lines()
        .rev()
        .find_map(|l| {
            let mut w = l.split_whitespace();
            (w.next() == Some("UID_MIN")).then(|| w.next()?.parse().ok()).flatten()
        })
        .unwrap_or(1000)
}

fn own_uid() -> u32 {
    fs::metadata("/proc/self").map_or(0, |m| m.uid())
}

/// Regular login accounts (not root, gdm, sddm, lightdm…).
pub fn is_regular_uid(uid: u32, uid_min: u32) -> bool {
    uid >= uid_min && uid != u32::MAX && uid != 65534
}

/// logind session classes the wizard may open in (`greeter`, `lock-screen`
/// and `background` sessions are excluded).
pub fn is_user_class(class: &str) -> bool {
    matches!(class, "user" | "user-early")
}

/// This process belongs to a regular user, and (when logind says so) to a
/// user session rather than a greeter.
pub fn regular_user_session() -> bool {
    let defs = fs::read_to_string("/etc/login.defs").unwrap_or_default();
    if !is_regular_uid(own_uid(), uid_min(&defs)) {
        return false;
    }
    match env::var("XDG_SESSION_CLASS") {
        Ok(class) if !class.is_empty() => is_user_class(&class),
        _ => true,
    }
}

fn runtime_dir() -> Option<PathBuf> {
    env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()).map(PathBuf::from)
}

fn lock_path() -> Option<PathBuf> {
    runtime_dir().map(|d| d.join("bindle-first-login.lock"))
}

/// Claim the right to open the wizard now; returns the token to hand to the
/// process that will release it. Autostart and the user service can both fire
/// in one session. Without `XDG_RUNTIME_DIR` there is no private place for a
/// lock, so this doesn't lock at all (`Some(None)`); `None` means another
/// launch holds it.
pub fn claim_launch() -> Option<Option<String>> {
    match lock_path() {
        Some(lock) => claim_lock(&lock).map(Some),
        None => Some(None),
    }
}

fn claim_lock(lock: &Path) -> Option<String> {
    let token = std::process::id().to_string();
    for _ in 0..2 {
        match fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(lock) {
            Ok(mut f) => {
                let _ = f.write_all(token.as_bytes());
                return Some(token);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // A crashed launch mustn't block forever: take over old locks.
                let stale = fs::symlink_metadata(lock)
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() >= LOCK_STALE);
                if !stale {
                    return None;
                }
                let _ = fs::remove_file(lock);
            }
            // Can't lock (read-only runtime dir?): open anyway rather than never.
            Err(_) => return Some(token),
        }
    }
    None
}

/// Remove the lock if it is still the one `token` claimed.
pub fn release_launch(token: Option<&str>) {
    if let (Some(lock), Some(token)) = (lock_path(), token) {
        release_lock(&lock, token);
    }
}

fn release_lock(lock: &Path, token: &str) {
    if fs::read_to_string(lock).is_ok_and(|held| held.trim() == token) {
        let _ = fs::remove_file(lock);
    }
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

/// `State`, `Type` and `Class` of a `loginctl show-session` dump.
pub fn parse_session(dump: &str) -> (bool, String, String) {
    let get = |key: &str| {
        dump.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).unwrap_or_default().trim().to_string()
    };
    (get("State") == "active", get("Type"), get("Class"))
}

/// Type of this user's active `user`-class session with a display, per logind
/// (`x11`, `wayland`, or `tty` for sessions greetd starts on a VT).
fn active_session_type() -> Option<String> {
    let user = env::var("USER").ok()?;
    let out = |args: &[&str]| -> Option<String> {
        let o = Command::new("loginctl").args(args).stderr(Stdio::null()).output().ok()?;
        let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
        (o.status.success() && !v.is_empty()).then_some(v)
    };
    let session = out(&["show-user", &user, "-p", "Display", "--value"])?;
    let (active, kind, class) = parse_session(&out(&["show-session", &session, "-p", "State,Type,Class"])?);
    (active && is_user_class(&class)).then_some(kind)
}

/// Wait for this user's graphical session (however long: the user manager may
/// start at a console or SSH login first), then open the wizard in it. Polls
/// only the cheap socket directories until a display appears; logind is asked
/// only then.
pub fn watch() -> Result<()> {
    if !regular_user_session() {
        return Ok(());
    }
    let Some(runtime) = runtime_dir() else { return Ok(()) };
    loop {
        if is_done() {
            return Ok(());
        }
        // Wayland sockets live in this user's private runtime dir, so any active
        // session will do (greetd sessions report `tty`). X11 sockets are global
        // (the greeter has one too): only trust one in an x11 session.
        if let Some((key, value)) = find_display(&runtime, Path::new("/tmp/.X11-unix"))
            && let Some(kind) = active_session_type()
            && (key == "WAYLAND_DISPLAY" || kind == "x11")
        {
            // Give the compositor/WM a moment to start mapping windows.
            std::thread::sleep(Duration::from_secs(6));
            let mut cmd = Command::new(env::current_exe()?);
            cmd.args(["first-login", "run"]).env(key, value).stdin(Stdio::null());
            if let Some(xauth) =
                env::var_os("HOME").map(|h| PathBuf::from(h).join(".Xauthority")).filter(|p| p.exists())
            {
                cmd.env("XAUTHORITY", xauth);
            }
            cmd.status().context("start the first-login wizard")?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_users_and_greeters_are_excluded() {
        assert_eq!(uid_min(""), 1000);
        assert_eq!(uid_min("# UID_MIN 5\nUID_MIN\t\t 2000\nUID_MAX 60000\n"), 2000);
        assert_eq!(uid_min("UID_MIN junk\n"), 1000);
        assert!(!is_regular_uid(0, 1000)); // root
        assert!(!is_regular_uid(120, 1000)); // gdm/sddm/lightdm
        assert!(!is_regular_uid(999, 1000));
        assert!(!is_regular_uid(65534, 1000)); // nobody
        assert!(is_regular_uid(1000, 1000) && is_regular_uid(1500, 1000));
        assert!(!is_regular_uid(1500, 2000));
        assert!(is_user_class("user") && !is_user_class("greeter") && !is_user_class("lock-screen"));
    }

    #[test]
    fn launch_lock_is_exclusive_and_owned() {
        let dir = crate::apply::sys::test_dir("lock");
        let lock = dir.join("bindle-first-login.lock");
        let token = claim_lock(&lock).unwrap();
        assert_eq!(claim_lock(&lock), None, "second launch is refused");
        release_lock(&lock, "someone-else");
        assert!(lock.exists(), "only the owner releases");
        release_lock(&lock, &token);
        assert!(!lock.exists());
        // A stale lock is taken over.
        fs::write(&lock, "1").unwrap();
        let old = SystemTime::now() - LOCK_STALE - Duration::from_secs(5);
        fs::File::options().write(true).open(&lock).unwrap().set_modified(old).unwrap();
        assert!(claim_lock(&lock).is_some());
    }

    #[test]
    fn parses_session_dump() {
        let (active, kind, class) = parse_session("Type=wayland\nClass=greeter\nState=active\n");
        assert!(active && kind == "wayland" && class == "greeter");
        assert!(!parse_session("State=online\n").0);
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
