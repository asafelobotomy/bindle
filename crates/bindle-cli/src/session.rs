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

/// The lock holds `<token> <owner pid>`; it is stale once the owner is gone.
/// The launcher hands the lock to the wizard in a new terminal and exits, so a
/// dead owner still counts for `LOCK_HANDOVER` while the terminal starts.
const LOCK_HANDOVER: Duration = Duration::from_secs(60);
/// Age after which an unreadable or ownerless (legacy) lock is taken over.
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
                let _ = f.write_all(format!("{token} {token}\n").as_bytes());
                return Some(token);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // A crashed launch mustn't block forever: take over dead locks.
                let age = fs::symlink_metadata(lock)
                    .and_then(|m| m.modified())
                    .map(|t| SystemTime::now().duration_since(t).unwrap_or_default())
                    .unwrap_or(LOCK_STALE);
                let held = fs::read_to_string(lock).unwrap_or_default();
                if !lock_stale(&held, age, pid_alive) {
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

/// `<token> <owner pid>`; a legacy lock holds only the launcher's pid.
fn parse_lock(held: &str) -> Option<(&str, u32)> {
    let mut w = held.split_whitespace();
    let token = w.next()?;
    let owner = w.next().unwrap_or(token).parse().ok()?;
    Some((token, owner))
}

fn lock_stale(held: &str, age: Duration, alive: impl Fn(u32) -> bool) -> bool {
    match parse_lock(held) {
        Some((_, owner)) if alive(owner) => false,
        Some(_) => age >= LOCK_HANDOVER,
        // Unreadable (or mid-write): only the old age limit applies.
        None => age >= LOCK_STALE,
    }
}

/// `pid` is a running process of this user (pids are reused, but rarely by
/// another process of the same user within one session).
fn pid_alive(pid: u32) -> bool {
    pid != 0 && fs::metadata(format!("/proc/{pid}")).is_ok_and(|m| m.uid() == own_uid())
}

/// The wizard's process takes over a lock its launcher claimed, so the lock
/// lives exactly as long as the wizard does.
pub fn adopt_launch(token: Option<&str>) {
    if let (Some(lock), Some(token)) = (lock_path(), token) {
        adopt_lock(&lock, token, std::process::id());
    }
}

fn adopt_lock(lock: &Path, token: &str, pid: u32) {
    let held = fs::read_to_string(lock).unwrap_or_default();
    if parse_lock(&held).is_none_or(|(t, _)| t != token) {
        return;
    }
    let tmp = lock.with_extension(format!("lock.{pid}"));
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(format!("{token} {pid}\n").as_bytes()));
    if written.is_err() || fs::rename(&tmp, lock).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Remove the lock if it is still the one `token` claimed.
pub fn release_launch(token: Option<&str>) {
    if let (Some(lock), Some(token)) = (lock_path(), token) {
        release_lock(&lock, token);
    }
}

fn release_lock(lock: &Path, token: &str) {
    if fs::read_to_string(lock).is_ok_and(|held| parse_lock(&held).is_some_and(|(t, _)| t == token)) {
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
        let set_age = |age: Duration| {
            let t = SystemTime::now() - age;
            fs::File::options().write(true).open(&lock).unwrap().set_modified(t).unwrap();
        };
        // A live owner keeps the lock however old it is (the wizard can run for hours).
        let token = claim_lock(&lock).unwrap();
        set_age(LOCK_STALE * 10);
        assert_eq!(claim_lock(&lock), None);
        // The wizard adopts it; the launcher's token still releases it.
        adopt_lock(&lock, "someone-else", 7);
        assert!(fs::read_to_string(&lock).unwrap().ends_with(&format!(" {token}\n")));
        adopt_lock(&lock, &token, 4_000_000_000);
        assert_eq!(fs::read_to_string(&lock).unwrap(), format!("{token} 4000000000\n"));
        // Dead owner: kept during the hand-over to the terminal, then taken over.
        assert_eq!(claim_lock(&lock), None);
        set_age(LOCK_HANDOVER + Duration::from_secs(5));
        assert!(claim_lock(&lock).is_some());
        // A legacy lock (launcher pid only) whose process is gone is taken over too.
        fs::write(&lock, "4000000001").unwrap();
        set_age(LOCK_HANDOVER + Duration::from_secs(5));
        assert!(claim_lock(&lock).is_some());
        // An unreadable lock falls back to the old age limit.
        fs::write(&lock, "garbage").unwrap();
        set_age(LOCK_HANDOVER + Duration::from_secs(5));
        assert_eq!(claim_lock(&lock), None);
        set_age(LOCK_STALE + Duration::from_secs(5));
        assert!(claim_lock(&lock).is_some());
    }

    #[test]
    fn lock_staleness() {
        let alive = |p: u32| p == 42;
        let fresh = Duration::from_secs(1);
        let old = LOCK_STALE * 100;
        assert!(!lock_stale("9 42\n", old, alive));
        assert!(!lock_stale("42", old, alive), "legacy lock with a live pid");
        assert!(!lock_stale("9 43", fresh, alive));
        assert!(lock_stale("9 43", LOCK_HANDOVER, alive));
        assert!(!lock_stale("", LOCK_HANDOVER, alive));
        assert!(lock_stale("x y", LOCK_STALE, alive));
        assert_eq!(parse_lock("12 34"), Some(("12", 34)));
        assert_eq!(parse_lock("12"), Some(("12", 12)));
        assert!(pid_alive(std::process::id()) && !pid_alive(0));
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
