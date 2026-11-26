//! The Recovery window's state, without Qt: what this computer can do
//! (Rollback needs Btrfs and snapper; Factory Restore an unencrypted disk),
//! the snapshots, and a run of `bindle rollback|rebase|restore` with its
//! output. The window runs the same commands as the terminal, so what they
//! do lives in one place (crates/bindle-cli/src/recover_cmd).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;
use serde_json::{Value, json};

/// What this computer allows, for the cards on the first page.
pub fn status() -> Value {
    bindle_core::recover::status::status()
}

/// The sample computer the screenshots show.
pub fn sample_status() -> Value {
    json!({ "rollback": null, "restore": null, "imageKept": false, "snapper": true })
}

pub fn sample_snapshots() -> Value {
    json!([
        { "number": 42, "type": "post", "pre-number": 41, "date": "2026-11-15 10:04:10", "description": "pacman -Syu" },
        { "number": 41, "type": "pre", "date": "2026-11-15 10:02:33", "description": "pacman -Syu" },
        { "number": 40, "type": "single", "date": "2026-11-15 09:00:01", "description": "timeline" },
        { "number": 37, "type": "single", "date": "2026-11-14 18:30:00", "description": "before bindle rebase" }
    ])
}

pub const SAMPLE_RUN: &[&str] = &[
    "Asking for administrator rights (once for this run).",
    "Rolling back makes snapshot 41 the system from the next start.",
    "Create a writable snapshot of '/run/bindle/rollback-top/@.snapshots/41/snapshot' in '/run/bindle/rollback-top/@'",
    "==> Building image from preset: /etc/mkinitcpio.d/linux.preset: 'default'",
    "==> Image generation successful",
    "Snapshot 41 is the system from the next start. The one before is kept as @.bindle-before-rollback-20261115-1104.",
];

/// A run's state, as the run page reads it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RunState {
    /// idle, running, done or failed.
    pub state: String,
    /// rollback, rebase, restore, keep-on, keep-off or cancel.
    pub kind: String,
    pub log: Vec<String>,
    /// The next start does something (rollback, restore): offer to restart.
    pub restart: bool,
}

/// The `bindle` arguments for an action: everything confirmed in the window.
pub fn argv(kind: &str, number: u32, dry_run: bool, sandbox: Option<&str>) -> Option<Vec<String>> {
    let mut a: Vec<String> = match kind {
        "rollback" => vec!["rollback".into(), number.to_string(), "--yes".into()],
        "rebase" => vec!["rebase".into(), "--yes".into()],
        "restore" => vec!["restore".into(), "--erase-confirmed".into()],
        "keep-on" => vec!["restore".into(), "--keep-image".into(), "on".into()],
        "keep-off" => vec!["restore".into(), "--keep-image".into(), "off".into()],
        "cancel" => vec!["restore".into(), "--cancel".into()],
        "snapshots" => vec!["rollback".into(), "--list".into(), "--json".into()],
        _ => return None,
    };
    if dry_run {
        a.push("--dry-run".into());
    }
    if let Some(dir) = sandbox {
        a.extend(["--sandbox".into(), dir.to_string()]);
    }
    Some(a)
}

/// Run `bindle` with `args`, each output line (stdout and stderr) to `line`.
/// Whether it succeeded.
pub fn run(exe: &Path, args: &[String], line: &mut dyn FnMut(String)) -> bool {
    let child = Command::new(exe).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            line(format!("couldn't start {}: {e}", exe.display()));
            return false;
        }
    };
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let stderr = child.stderr.take().map(|err| {
        let tx = tx.clone();
        std::thread::spawn(move || BufReader::new(err).lines().map_while(Result::ok).for_each(|l| drop(tx.send(l))))
    });
    if let Some(out) = child.stdout.take() {
        std::thread::spawn(move || BufReader::new(out).lines().map_while(Result::ok).for_each(|l| drop(tx.send(l))));
    } else {
        drop(tx);
    }
    for l in rx {
        line(l);
    }
    if let Some(h) = stderr {
        let _ = h.join();
    }
    child.wait().is_ok_and(|s| s.success())
}

/// The `bindle` program beside this window (or on PATH).
pub fn bindle() -> PathBuf {
    crate::apply::helper_exe().unwrap_or_else(|| PathBuf::from("bindle"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_is_confirmed_in_the_window() {
        assert_eq!(argv("rollback", 7, false, None).unwrap(), ["rollback", "7", "--yes"]);
        assert_eq!(
            argv("restore", 0, true, Some("/s")).unwrap(),
            ["restore", "--erase-confirmed", "--dry-run", "--sandbox", "/s"]
        );
        assert!(argv("rm -rf", 0, false, None).is_none());
    }

    #[test]
    fn a_run_collects_both_streams() {
        let mut lines = vec![];
        let ok = run(Path::new("sh"), &["-c".into(), "echo out; echo err >&2".into()], &mut |l| lines.push(l));
        lines.sort();
        assert!(ok && lines == ["err", "out"]);
        assert!(!run(Path::new("sh"), &["-c".into(), "exit 3".into()], &mut |_| ()));
    }
}
