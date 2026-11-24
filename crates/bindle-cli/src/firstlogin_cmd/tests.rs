//! Signals end a process, so these run the hand-off in a child: this test
//! binary again, running one of the `child_*` tests (ignored otherwise).
use super::*;
use crate::uq_handoff::defer_path;
use crate::uq_handoff::tests::{STUB_ENV, fake_uqctl, tempdir, wait_calls};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Set in the child: its scratch directory (the runtime dir, too).
const CHILD_ENV: &str = "BINDLE_TEST_HANDOFF_DIR";

fn spawn_child(test: &str, dir: &Path) -> std::process::Child {
    Command::new(std::env::current_exe().unwrap())
        .args([&format!("firstlogin_cmd::tests::{test}"), "--exact", "--ignored", "--test-threads=1"])
        .env(CHILD_ENV, dir)
        .env("XDG_RUNTIME_DIR", dir)
        .env("XDG_STATE_HOME", dir.join("state"))
        .env(STUB_ENV, fake_uqctl(dir))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(Instant::now() < deadline, "{} never appeared", path.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn closing_the_terminal_still_hands_off() {
    for signal in [libc::SIGHUP, libc::SIGTERM, libc::SIGINT] {
        let t = tempdir();
        let mut child = spawn_child("child_holds_the_handoff", t.path());
        wait_for(&t.path().join("ready"));
        assert!(defer_path(t.path()).is_file(), "held while the wizard runs");
        // SAFETY: kill(2) on our own child's pid; no memory is passed.
        assert_eq!(unsafe { libc::kill(child.id() as libc::pid_t, signal) }, 0);
        let status = child.wait().unwrap();
        assert_eq!(status.signal(), Some(signal), "ends as the signal would have: {status:?}");
        assert!(!defer_path(t.path()).exists(), "signal {signal}: defer file left behind");
        assert_eq!(wait_calls(t.path()).trim(), "welcome", "signal {signal}");
    }
}

#[test]
fn returning_early_in_the_terminal_still_hands_off() {
    let t = tempdir();
    // The launcher's file, left for this process (it disarmed).
    std::fs::create_dir_all(defer_path(t.path()).parent().unwrap()).unwrap();
    std::fs::write(defer_path(t.path()), "bindle\n").unwrap();
    // Already done: `run` returns before the wizard.
    std::fs::create_dir_all(t.path().join("state/bindle")).unwrap();
    std::fs::write(t.path().join("state/bindle/first-login-done"), "").unwrap();
    let status = spawn_child("child_returns_early", t.path()).wait().unwrap();
    assert!(status.success(), "{status:?}");
    assert!(!defer_path(t.path()).exists());
    assert_eq!(wait_calls(t.path()).trim(), "welcome");
}

#[test]
#[ignore = "run in a child by closing_the_terminal_still_hands_off"]
fn child_holds_the_handoff() {
    let Some(dir) = std::env::var_os(CHILD_ENV).map(PathBuf::from) else { return };
    let _held = hold_handoff().expect("hand-off held");
    std::fs::write(dir.join("ready"), "").unwrap();
    // The wizard, until the signal ends this process.
    std::thread::sleep(Duration::from_secs(30));
    panic!("no signal arrived");
}

#[test]
#[ignore = "run in a child by returning_early_in_the_terminal_still_hands_off"]
fn child_returns_early() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    run(&Catalog::arch().unwrap(), &Facts::default(), true, None).unwrap();
}
