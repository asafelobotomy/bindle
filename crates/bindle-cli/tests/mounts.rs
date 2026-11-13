//! Drives mounted at startup, end to end through the root helper in
//! `--fake-commands` mode: nothing here touches the real /etc/fstab.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const FSTAB: &str = "# /dev/nvme0n1p2\nUUID=root-1\t/\text4\trw,relatime\t0 1\n";

/// Send `requests` to one helper session; returns its events.
fn helper(root: &Path, requests: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bindle"))
        .args(["helper", "--fake-commands", "--root", root.to_str().unwrap(), "--log", "/var/log/bindle/test.log"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for r in requests.iter().chain(&[json!({ "op": "quit" })]) {
        writeln!(stdin, "{r}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn entry(uuid: &str, dir: &str, mount_now: bool) -> Value {
    json!({ "uuid": uuid, "dir": dir, "fstype": "ext4", "options": "defaults,nofail", "pass": 2, "mount_now": mount_now })
}

fn mount(entries: &[Value]) -> Value {
    json!({ "op": "run", "step": { "kind": "mount-at-boot", "entries": entries } })
}

fn outputs(events: &[Value]) -> Vec<&str> {
    events.iter().filter(|e| e["event"] == "output").map(|e| e["line"].as_str().unwrap()).collect()
}

#[test]
fn drives_are_added_to_fstab_once_and_undo_puts_it_back() {
    let root = std::env::temp_dir().join(format!("bindle-cli-mounts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("etc")).unwrap();
    fs::write(root.join("etc/fstab"), FSTAB).unwrap();
    let fstab = || fs::read_to_string(root.join("etc/fstab")).unwrap();

    let snapshot = json!({ "op": "snapshot", "plan": { "label": "t", "user": "u", "files": ["/etc/fstab"], "units": [],
                "packages": [], "aur": [], "dirs": ["/mnt/Data", "/mnt/Games"] } });
    let both = [entry("aaaa-1", "/mnt/Data", true), entry("bbbb-2", "/mnt/Games", false)];
    let events = helper(&root, &[snapshot, mount(&both)]);
    let id = events.iter().find(|e| e["event"] == "snapshot").unwrap()["id"].as_str().unwrap().to_string();
    let lines = outputs(&events);
    assert!(lines.contains(&"would run: systemctl daemon-reload"), "{lines:?}");
    // Only the drive that isn't mounted yet is mounted now.
    assert!(lines.contains(&"would run: mount -- /mnt/Data"));
    assert_eq!(lines.iter().filter(|l| l.starts_with("would run: mount ")).count(), 1);

    let written = fstab();
    assert!(written.starts_with(FSTAB), "lines outside the block are untouched:\n{written}");
    assert!(written.contains("UUID=aaaa-1\t/mnt/Data\text4\tdefaults,nofail\t0 2\n"));
    assert!(root.join("mnt/Data").is_dir() && root.join("mnt/Games").is_dir());

    // Again, and with one drive's options changed: still one line per drive.
    let mut changed = entry("aaaa-1", "/mnt/Data", false);
    changed["options"] = json!("defaults,noatime,nofail");
    helper(&root, &[mount(&both), mount(&[changed])]);
    let again = fstab();
    assert_eq!(again.matches("UUID=aaaa-1").count(), 1);
    assert_eq!(again.matches("# BEGIN Managed by Bindle").count(), 1);
    assert!(again.contains("defaults,noatime,nofail") && again.contains("UUID=bbbb-2"));

    // Entries that could smuggle in another line or path are refused.
    let bad = [
        entry("aaaa-1", "/mnt/../etc", false),
        entry("a b", "/mnt/x", false),
        json!({ "uuid": "c", "dir": "/mnt/c", "fstype": "ext4", "options": "defaults\n/dev/sda1 / ext4", "pass": 0 }),
    ];
    for b in bad {
        let events = helper(&root, &[mount(&[b])]);
        assert!(events.iter().any(|e| e["event"] == "failed"), "{events:?}");
    }
    assert_eq!(fstab(), again);

    let events = helper(&root, &[json!({ "op": "restore", "id": id })]);
    assert!(outputs(&events).contains(&"restored /etc/fstab"), "{events:?}");
    assert_eq!(fstab(), FSTAB);
    assert!(!root.join("mnt/Data").exists() && !root.join("mnt/Games").exists(), "empty mount points go again");
    let _ = fs::remove_dir_all(&root);
}
