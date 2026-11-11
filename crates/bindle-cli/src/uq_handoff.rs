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

use std::ffi::OsString;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError, mpsc};

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

pub const UQCTL: &str = "/usr/bin/uqctl";

/// Where the defer file lives for this runtime dir.
pub fn defer_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("uq").join("welcome.defer")
}

/// A held hand-off: `finish` removes the defer file and opens Welcome.
/// `disarm` keeps the file (another process takes the hand-off over).
pub struct Handoff {
    path: PathBuf,
    /// `uqctl`, or (in tests) `/bin/sh <stub>`; `welcome` is appended.
    uqctl: Vec<OsString>,
    armed: bool,
}

impl Handoff {
    /// Hold the defer file when uQ is installed. Creating it again refreshes
    /// its age, so the process that ends the wizard can call this too.
    pub fn begin() -> Option<Handoff> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty())?;
        // Tests that run this in a child process stand in a script for uqctl.
        #[cfg(test)]
        if let Some(stub) = std::env::var_os(tests::STUB_ENV) {
            return Handoff::hold(Path::new(&runtime), vec!["/bin/sh".into(), stub]);
        }
        Handoff::begin_in(Path::new(&runtime), Path::new(UQCTL))
    }

    pub fn begin_in(runtime_dir: &Path, uqctl: &Path) -> Option<Handoff> {
        if !uqctl.is_file() {
            return None;
        }
        Handoff::hold(runtime_dir, vec![uqctl.into()])
    }

    fn hold(runtime_dir: &Path, uqctl: Vec<OsString>) -> Option<Handoff> {
        if !runtime_dir.is_dir() {
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
        Some(Handoff { path, uqctl, armed: true })
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

    /// Also finish when this process is told to end: closing the terminal
    /// window sends SIGHUP (logging out SIGTERM, an interrupt SIGINT), and
    /// then nothing is dropped. A thread waits for the signal (the handler
    /// itself only wakes it), finishes, and ends the process as the signal
    /// would have.
    pub fn finish_on_signals(self) -> Held {
        let slot = Arc::new(Mutex::new(Some(self)));
        let shared = Arc::clone(&slot);
        let (ready, registered) = mpsc::channel();
        let _ = std::thread::Builder::new().name("uq-handoff".into()).spawn(move || {
            // Never dropped once registered: unregistering would leave the
            // signals ignored rather than back to their default.
            let Ok(mut signals) = Signals::new([SIGHUP, SIGTERM, SIGINT]) else { return };
            let _ = ready.send(());
            if let Some(signal) = signals.forever().next() {
                finish_slot(&shared);
                let _ = signal_hook::low_level::emulate_default_handler(signal);
                std::process::exit(128 + signal);
            }
        });
        // Registered before the wizard starts, so no signal slips past. (If
        // the thread or the registration fails, the sender is gone at once.)
        let _ = registered.recv();
        Held(slot)
    }

    fn release(&self, open: bool) {
        let _ = fs::remove_file(&self.path);
        if let (true, Some((prog, args))) = (open, self.uqctl.split_first()) {
            // Its own process group: this process may be ending, and when a
            // session leader exits its foreground group gets SIGHUP.
            let _ = Command::new(prog)
                .args(args)
                .arg("welcome")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
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

/// A hand-off that finishes when dropped or when a signal ends the process,
/// whichever comes first (see [`Handoff::finish_on_signals`]).
pub struct Held(Arc<Mutex<Option<Handoff>>>);

impl Drop for Held {
    fn drop(&mut self) {
        finish_slot(&self.0);
    }
}

/// Finish with the lock held, so a signal arriving meanwhile waits for it.
fn finish_slot(slot: &Mutex<Option<Handoff>>) {
    let mut held = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(h) = held.take() {
        h.finish();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// Set in a child test process: the stub script that stands in for uqctl.
    pub(crate) const STUB_ENV: &str = "BINDLE_TEST_UQCTL";

    /// A fresh directory, removed when dropped.
    pub(crate) struct Tmp(PathBuf);
    impl Tmp {
        pub(crate) fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    pub(crate) fn tempdir() -> Tmp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("bindle-uq-handoff-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }

    /// A stub uqctl that logs its arguments to `calls`. It is run as
    /// `/bin/sh <stub>`, never exec'd itself: another test's fork can still
    /// hold the write handle, and exec would then fail with ETXTBSY.
    pub(crate) fn fake_uqctl(dir: &Path) -> PathBuf {
        let p = dir.join("uqctl");
        fs::write(&p, format!("echo \"$@\" >> '{}'\n", dir.join("calls").display())).unwrap();
        p
    }

    fn hold_with_stub(dir: &Path) -> Option<Handoff> {
        Handoff::hold(dir, vec!["/bin/sh".into(), fake_uqctl(dir).into()])
    }

    /// What the stub logged, once it has logged something (or after 5 s).
    pub(crate) fn wait_calls(dir: &Path) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let calls = fs::read_to_string(dir.join("calls")).unwrap_or_default();
            if !calls.is_empty() || Instant::now() > deadline {
                return calls;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn without_uq_nothing_is_written() {
        let t = tempdir();
        assert!(Handoff::begin_in(t.path(), &t.path().join("missing")).is_none());
        assert!(!defer_path(t.path()).exists());
    }

    #[test]
    fn begin_holds_the_file_and_finish_opens_welcome() {
        let t = tempdir();
        let h = hold_with_stub(t.path()).unwrap();
        assert!(defer_path(t.path()).is_file());
        h.finish();
        assert!(!defer_path(t.path()).exists());
        assert_eq!(wait_calls(t.path()).trim(), "welcome");
    }

    #[test]
    fn disarm_keeps_the_file_for_the_next_process() {
        let t = tempdir();
        hold_with_stub(t.path()).unwrap().disarm();
        assert!(defer_path(t.path()).is_file());
        std::thread::sleep(Duration::from_millis(100));
        assert!(!t.path().join("calls").exists(), "Welcome not opened yet");
    }

    #[test]
    fn dropping_still_hands_off() {
        let t = tempdir();
        drop(hold_with_stub(t.path()).unwrap());
        assert!(!defer_path(t.path()).exists());
        assert_eq!(wait_calls(t.path()).trim(), "welcome");
    }

    #[test]
    fn a_link_in_place_of_the_file_is_left_alone() {
        let t = tempdir();
        fs::create_dir_all(t.path().join("uq")).unwrap();
        let target = t.path().join("elsewhere");
        std::os::unix::fs::symlink(&target, defer_path(t.path())).unwrap();
        assert!(hold_with_stub(t.path()).is_none());
        assert!(!target.exists());
    }
}
