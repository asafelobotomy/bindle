//! The hand-off to uQ's Welcome (uQ Phase 5 spec §5.2, uq `docs/welcome.md`).
//!
//! While the first-login wizard is open in a uQ session, Bindle holds
//! `$XDG_RUNTIME_DIR/uq/welcome.defer`, so uQ doesn't open its Welcome window
//! on top of the wizard. When the wizard ends, Bindle removes the file and runs
//! `uqctl welcome`, which opens Welcome once (and does nothing if the user has
//! already finished it). uQ ignores a defer file older than 30 minutes, so a
//! crashed wizard can't hide Welcome for good.
//!
//! Without uQ (`uqctl` not installed) or without `XDG_RUNTIME_DIR`, none of
//! this happens.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const UQCTL: &str = "/usr/bin/uqctl";

/// Where the defer file lives for this runtime dir.
pub fn defer_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("uq").join("welcome.defer")
}

/// A held hand-off: `finish` removes the defer file and opens Welcome.
/// `disarm` keeps the file (another process takes the hand-off over).
pub struct Handoff {
    path: PathBuf,
    uqctl: PathBuf,
    armed: bool,
}

impl Handoff {
    /// Hold the defer file when uQ is installed. Creating it again refreshes
    /// its age, so the process that ends the wizard can call this too.
    pub fn begin() -> Option<Handoff> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty())?;
        Handoff::begin_in(Path::new(&runtime), Path::new(UQCTL))
    }

    pub fn begin_in(runtime_dir: &Path, uqctl: &Path) -> Option<Handoff> {
        if !uqctl.is_file() || !runtime_dir.is_dir() {
            return None;
        }
        let path = defer_path(runtime_dir);
        let dir = path.parent()?;
        if fs::create_dir_all(dir).is_err() {
            return None;
        }
        // Never follow a link someone left there; a plain file is all uQ needs.
        if fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file()) {
            return None;
        }
        fs::write(&path, b"bindle\n").ok()?;
        Some(Handoff { path, uqctl: uqctl.to_path_buf(), armed: true })
    }

    /// Leave the file in place: the wizard continues in another process
    /// (a terminal), which takes the hand-off over.
    pub fn disarm(mut self) {
        self.armed = false;
    }

    /// Remove the defer file, then ask uQ to open Welcome.
    pub fn finish(mut self) {
        self.armed = false;
        self.release(true);
    }

    fn release(&self, open: bool) {
        let _ = fs::remove_file(&self.path);
        if open {
            let _ = Command::new(&self.uqctl)
                .arg("welcome")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }
}

impl Drop for Handoff {
    /// An early return or an error still lets Welcome open later.
    fn drop(&mut self) {
        if self.armed {
            self.release(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh directory, removed when dropped.
    struct Tmp(PathBuf);
    impl Tmp {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn tempdir() -> Tmp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("bindle-uq-handoff-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }

    fn fake_uqctl(dir: &Path) -> PathBuf {
        let p = dir.join("uqctl");
        let log = dir.join("calls");
        fs::write(&p, format!("#!/bin/sh\necho \"$@\" >> {}\n", log.display())).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn wait_calls(dir: &Path) -> String {
        for _ in 0..100 {
            if let Ok(s) = fs::read_to_string(dir.join("calls")) {
                return s;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        String::new()
    }

    #[test]
    fn without_uq_nothing_is_written() {
        let t = tempdir();
        assert!(Handoff::begin_in(t.path(), &t.path().join("missing")).is_none());
        assert!(!defer_path(t.path()).exists());
    }

    #[test]
    fn begin_holds_the_file_and_finish_opens_welcome() {
        let _lock = crate::gui::script_lock();
        let t = tempdir();
        let uqctl = fake_uqctl(t.path());
        let h = Handoff::begin_in(t.path(), &uqctl).unwrap();
        assert!(defer_path(t.path()).is_file());
        h.finish();
        assert!(!defer_path(t.path()).exists());
        assert_eq!(wait_calls(t.path()).trim(), "welcome");
    }

    #[test]
    fn disarm_keeps_the_file_for_the_next_process() {
        let _lock = crate::gui::script_lock();
        let t = tempdir();
        let uqctl = fake_uqctl(t.path());
        Handoff::begin_in(t.path(), &uqctl).unwrap().disarm();
        assert!(defer_path(t.path()).is_file());
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!t.path().join("calls").exists(), "Welcome not opened yet");
    }

    #[test]
    fn dropping_still_hands_off() {
        let _lock = crate::gui::script_lock();
        let t = tempdir();
        let uqctl = fake_uqctl(t.path());
        drop(Handoff::begin_in(t.path(), &uqctl).unwrap());
        assert!(!defer_path(t.path()).exists());
        assert_eq!(wait_calls(t.path()).trim(), "welcome");
    }

    #[test]
    fn a_link_in_place_of_the_file_is_left_alone() {
        let _lock = crate::gui::script_lock();
        let t = tempdir();
        let uqctl = fake_uqctl(t.path());
        fs::create_dir_all(t.path().join("uq")).unwrap();
        let target = t.path().join("elsewhere");
        std::os::unix::fs::symlink(&target, defer_path(t.path())).unwrap();
        assert!(Handoff::begin_in(t.path(), &uqctl).is_none());
        assert!(!target.exists());
    }
}
