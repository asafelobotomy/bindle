//! End-to-end tests of the `bindle` binary. Nothing here touches the real
//! system: apply/undo run with `--sandbox`, the helper with `--fake-commands`,
//! and `iso` with `--dry-run`. Detection still reads this machine, so the
//! assertions only rely on modules without hardware conditions.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PACMAN_CONF: &str =
    "[options]\nHoldPkg = pacman glibc\n#Color\n\n#[multilib]\n#Include = /etc/pacman.d/mirrorlist\n";

fn bindle(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bindle")).args(args).stdin(Stdio::null()).output().unwrap()
}

fn ok(args: &[&str]) -> String {
    let out = bindle(args);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "bindle {args:?} failed:\n{text}\n{}", String::from_utf8_lossy(&out.stderr));
    text
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bindle-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sandbox(name: &str) -> PathBuf {
    let root = scratch(name);
    fs::create_dir_all(root.join("etc")).unwrap();
    fs::write(root.join("etc/pacman.conf"), PACMAN_CONF).unwrap();
    root
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn list_and_plan_json() {
    assert!(ok(&["list"]).contains("Profiles:"));
    let plan: serde_json::Value =
        serde_json::from_str(&ok(&["plan", "--json", "--profile", "minimal", "--with", "printing"])).unwrap();
    let modules = plan["modules"].as_array().unwrap();
    assert!(modules.iter().any(|m| m == "printing"), "{plan}");
    assert!(plan["steps"].is_array());
    assert!(!bindle(&["plan", "--profile", "nope"]).status.success());
}

#[test]
fn selection_files_layer_under_flags() {
    let dir = scratch("from");
    let file = dir.join("selection.toml");
    fs::write(&file, "profile = \"minimal\"\nwith = [\"printing\", \"archives\"]\n").unwrap();
    let plan: serde_json::Value =
        serde_json::from_str(&ok(&["plan", "--json", "--from", s(&file), "--without", "archives"])).unwrap();
    let modules = plan["modules"].as_array().unwrap();
    assert!(modules.iter().any(|m| m == "printing") && !modules.iter().any(|m| m == "archives"), "{plan}");
}

#[test]
fn sandbox_apply_then_undo_restores_files() {
    let root = sandbox("apply");
    let out = ok(&["apply", "--sandbox", s(&root), "--yes", "--profile", "minimal", "--with", "printing"]);
    assert!(out.contains("steps applied"), "{out}");
    assert_ne!(fs::read_to_string(root.join("etc/pacman.conf")).unwrap(), PACMAN_CONF, "pacman-qol edits it");
    assert!(ok(&["undo", "--sandbox", s(&root), "--list"]).contains(" files, "));
    ok(&["undo", "--sandbox", s(&root), "--yes"]);
    assert_eq!(fs::read_to_string(root.join("etc/pacman.conf")).unwrap(), PACMAN_CONF);
}

#[test]
fn apply_without_a_terminal_needs_yes() {
    let root = sandbox("noyes");
    let out = bindle(&["apply", "--sandbox", s(&root), "--profile", "minimal"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--yes"));
}

#[test]
fn iso_dry_run_writes_a_private_workdir() {
    let work = scratch("iso").join("work");
    let out = ok(&["iso", "--dry-run", "--yes", "--profile", "minimal", "--desktop", "plasma", "--workdir", s(&work)]);
    assert!(out.contains("Would run:"), "{out}");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(work.join("bindle-archinstall.json")).unwrap()).unwrap();
    assert!(config.is_object());
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(work.join("bindle-archinstall.json")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

/// The root helper answers garbage with an error event and keeps serving.
#[test]
fn helper_rejects_malformed_and_unsafe_requests() {
    let root = sandbox("helper");
    let mut child = Command::new(env!("CARGO_BIN_EXE_bindle"))
        .args(["helper", "--fake-commands", "--root", s(&root), "--log", "/var/log/bindle/test.log"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let requests = [
        "not json",
        r#"{"op":"run","step":{"kind":"install-packages","packages":["--root=/"]}}"#,
        r#"{"op":"run","step":{"kind":"write-file","module":"x","file":{"path":"/etc/../root/x","content":"","mode":420}}}"#,
        r#"{"op":"run","step":{"kind":"edit-pacman-conf","options":{"Color\n[evil]":""},"enable_repos":[]}}"#,
        r#"{"op":"run","step":{"kind":"run-as-user","module":"x","argv":["true"]}}"#,
        r#"{"op":"restore","id":"../../etc"}"#,
        r#"{"op":"run","step":{"kind":"enable-units","units":["fstrim.timer"]}}"#,
        r#"{"op":"quit"}"#,
    ];
    let mut stdin = child.stdin.take().unwrap();
    for r in requests {
        writeln!(stdin, "{r}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let events: Vec<serde_json::Value> =
        String::from_utf8_lossy(&out.stdout).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let kinds: Vec<&str> = events.iter().map(|e| e["event"].as_str().unwrap()).filter(|k| *k != "output").collect();
    assert_eq!(kinds, ["hello", "failed", "failed", "failed", "failed", "failed", "failed", "done"], "{events:?}");
    assert!(out.status.success());
}
