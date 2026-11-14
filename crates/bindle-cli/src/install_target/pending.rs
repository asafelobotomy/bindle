//! At first login, in the user's session: the user steps the install from the
//! ISO saved for later (they need a running session). Every login hook ends
//! up here (`bindle first-login run`): the install leaves setup unmarked
//! while steps wait, so the user unit (Sway, niri, i3…) and the console hook
//! fire too. Steps that fail are kept for the next login and reported.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use crate::firstlogin;

/// User steps left for first login, under the user's state directory.
pub const PENDING: &str = "finish-at-login.jsonl";
/// The install from the ISO finished everything but these steps: once they
/// have all run, setup counts as finished (no wizard).
pub const SET_UP: &str = "set-up-from-iso";
const DONE: &str = "first-login-done";
/// Another login hook running the steps can hold its claim this long.
const WAIT: Duration = Duration::from_secs(10 * 60);

pub fn finish_pending() {
    let failed = finish_in(&firstlogin::state_dirs(), &mut |argv| {
        let (prog, args) = argv.split_first()?;
        Command::new(prog).args(args).stdin(Stdio::null()).status().ok()?.code()
    });
    if !failed.is_empty() {
        report(&failed);
    }
}

/// On the console and in the notification area (when notify-send exists).
fn report(failed: &[String]) {
    let body = format!(
        "These setup steps didn't finish: {}. They run again at your next login; run `bindle` to see what's left.",
        failed.join("; ")
    );
    eprintln!("[bindle] {body}");
    let _ = Command::new("notify-send")
        .args(["--app-name=Bindle", "--icon=bindle", "Some setup steps didn't finish", &body])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Wait while another login hook runs the steps (its claim exists and is
/// recent), so this one doesn't open the wizard before they are done.
fn wait_for_claims(dirs: &[PathBuf]) {
    let fresh = |p: &Path| {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() < WAIT)
    };
    while dirs.iter().any(|d| fresh(&d.join(format!("{PENDING}.running")))) {
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Run the saved steps in each state directory (`run` gives a command's exit
/// code). Returns the commands that failed; their lines stay for next time.
fn finish_in(dirs: &[PathBuf], run: &mut dyn FnMut(&[String]) -> Option<i32>) -> Vec<String> {
    let mut failed = Vec::new();
    let mut ran = false;
    for dir in dirs {
        // Claimed by renaming: autostart and the user unit can both fire.
        let claimed = dir.join(format!("{PENDING}.running"));
        if fs::rename(dir.join(PENDING), &claimed).is_err() {
            continue;
        }
        ran = true;
        let text = fs::read_to_string(&claimed).unwrap_or_default();
        let mut keep = String::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let Ok(bindle_core::Step::RunAsUser { argv, manual, .. }) = serde_json::from_str(line) else {
                eprintln!("[bindle] skipped a saved setup step it can't read: {line}");
                continue;
            };
            let ok = run(&argv).is_some_and(|c| c == 0 || manual.contains_key(&c.to_string()));
            if !ok {
                failed.push(argv.join(" "));
                keep.push_str(line);
                keep.push('\n');
            }
        }
        if !keep.is_empty() {
            let saved = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(dir.join(PENDING))
                .and_then(|mut f| f.write_all(keep.as_bytes()));
            if let Err(e) = saved {
                eprintln!("[bindle] couldn't keep the steps that failed for next time: {e}");
            }
        }
        let _ = fs::remove_file(&claimed);
    }
    if !ran {
        wait_for_claims(dirs);
        return failed;
    }
    if failed.is_empty() && dirs.iter().any(|d| d.join(SET_UP).exists()) {
        for d in dirs {
            let _ = fs::create_dir_all(d);
            if let Err(e) = fs::write(d.join(DONE), "") {
                eprintln!("[bindle] couldn't mark setup as finished: {e}");
            }
            let _ = fs::remove_file(d.join(SET_UP));
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn step(argv: &[&str], manual: &[(&str, &str)]) -> String {
        let manual: BTreeMap<String, String> = manual.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let argv = argv.iter().map(|a| a.to_string()).collect();
        serde_json::to_string(&bindle_core::Step::RunAsUser { module: "m".into(), argv, manual }).unwrap()
    }

    fn dirs(name: &str) -> Vec<PathBuf> {
        let base = crate::apply::sys::test_dir(name);
        let d = vec![base.join("xdg/bindle"), base.join("home/.local/state/bindle")];
        d.iter().for_each(|d| fs::create_dir_all(d).unwrap());
        d
    }

    /// Exit codes by program name; anything else succeeds.
    fn fake(codes: &[(&str, i32)]) -> impl FnMut(&[String]) -> Option<i32> {
        let codes: Vec<(String, i32)> = codes.iter().map(|(p, c)| (p.to_string(), *c)).collect();
        move |argv| Some(codes.iter().find(|(p, _)| *p == argv[0]).map_or(0, |(_, c)| *c))
    }

    #[test]
    fn all_steps_run_and_setup_counts_as_finished() {
        let d = dirs("pending-ok");
        let lines = [step(&["systemctl", "--user", "enable", "x"], &[]), step(&["takeover"], &[("3", "log out")])];
        fs::write(d[1].join(PENDING), lines.join("\n") + "\n").unwrap();
        fs::write(d[1].join(SET_UP), "").unwrap();
        let mut seen = Vec::new();
        let mut run = fake(&[("takeover", 3)]);
        let failed = finish_in(&d, &mut |argv| {
            seen.push(argv.join(" "));
            run(argv)
        });
        assert!(failed.is_empty(), "exit 3 is a known manual step: {failed:?}");
        assert_eq!(seen, ["systemctl --user enable x", "takeover"]);
        assert!(d.iter().all(|d| d.join(DONE).exists() && !d.join(SET_UP).exists()));
        assert!(!d[1].join(PENDING).exists() && !d[1].join(format!("{PENDING}.running")).exists());
        // Nothing left: the next login runs nothing.
        assert!(finish_in(&d, &mut |_| panic!("ran again")).is_empty());
    }

    #[test]
    fn failed_steps_are_kept_and_setup_stays_unfinished() {
        let d = dirs("pending-fail");
        let (good, bad) = (step(&["good"], &[]), step(&["bad", "arg"], &[]));
        fs::write(d[1].join(PENDING), format!("{good}\n\n{bad}\nnot json\n")).unwrap();
        fs::write(d[1].join(SET_UP), "").unwrap();
        assert_eq!(finish_in(&d, &mut fake(&[("bad", 1)])), ["bad arg"]);
        assert_eq!(fs::read_to_string(d[1].join(PENDING)).unwrap(), format!("{bad}\n"));
        assert!(d.iter().all(|d| !d.join(DONE).exists()), "the wizard opens to finish");
        assert!(d[1].join(SET_UP).exists());
        // A command that can't start counts as failed too.
        assert_eq!(finish_in(&d, &mut |_| None), ["bad arg"]);
        // It works at a later login: now setup is finished.
        assert!(finish_in(&d, &mut fake(&[])).is_empty());
        assert!(d.iter().all(|d| d.join(DONE).exists()) && !d[1].join(PENDING).exists());
    }

    #[test]
    fn steps_left_by_a_failed_install_never_mark_setup_finished() {
        let d = dirs("pending-no-setup");
        fs::write(d[1].join(PENDING), step(&["ok"], &[])).unwrap();
        assert!(finish_in(&d, &mut fake(&[])).is_empty());
        assert!(d.iter().all(|d| !d.join(DONE).exists()), "install-target didn't finish: the wizard opens");
    }
}
