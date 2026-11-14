use super::*;

#[test]
fn snapshot_and_restore_round_trip() {
    let root = crate::sys::test_dir("snapshot");
    fs::create_dir_all(root.join("etc")).unwrap();
    fs::write(root.join("etc/pacman.conf"), "original\n").unwrap();
    let plan = SnapshotPlan {
        files: vec!["/etc/pacman.conf".into(), "/etc/new.conf".into()],
        units: vec!["fstrim.timer".into()],
        ..Default::default()
    };
    let m = create(&root, &plan).unwrap();
    assert!(m.files[0].existed && !m.files[1].existed);
    // What the apply does: write, then record what it wrote.
    fs::write(root.join("etc/pacman.conf"), "changed\n").unwrap();
    fs::write(root.join("etc/new.conf"), "new\n").unwrap();
    record_written(&root, &m.id, "/etc/pacman.conf").unwrap();
    record_written(&root, &m.id, "/etc/new.conf").unwrap();

    let loaded = &list(&root)[0];
    assert_eq!(loaded.id, m.id);
    let r = restore_files(&root, loaded, false);
    assert!(r.errors.is_empty() && r.kept.is_empty(), "{r:?}");
    assert_eq!(fs::read_to_string(root.join("etc/pacman.conf")).unwrap(), "original\n");
    assert!(!root.join("etc/new.conf").exists());
    assert!(load(&root, "../etc").is_err());

    // A package created the file after the snapshot: undo keeps that version.
    let m = create(&root, &plan).unwrap();
    fs::write(root.join("etc/new.conf"), "from package\n").unwrap();
    capture_late(&root, &m.id, "/etc/new.conf").unwrap();
    fs::write(root.join("etc/new.conf"), "bindle\n").unwrap();
    record_written(&root, &m.id, "/etc/new.conf").unwrap();
    // A second write of the same file: the original is still the package's.
    capture_late(&root, &m.id, "/etc/new.conf").unwrap();
    fs::write(root.join("etc/new.conf"), "bindle again\n").unwrap();
    record_written(&root, &m.id, "/etc/new.conf").unwrap();
    assert!(restore_files(&root, &load(&root, &m.id).unwrap(), false).errors.is_empty());
    assert_eq!(fs::read_to_string(root.join("etc/new.conf")).unwrap(), "from package\n");
}

#[test]
fn undo_keeps_files_changed_since_bindle_wrote_them() {
    let root = crate::sys::test_dir("snapshot-kept");
    fs::create_dir_all(root.join("etc")).unwrap();
    for (f, text) in [("a.conf", "a\n"), ("b.conf", "b\n"), ("untouched.conf", "u\n")] {
        fs::write(root.join("etc").join(f), text).unwrap();
    }
    let files = ["/etc/a.conf", "/etc/b.conf", "/etc/new.conf", "/etc/gone.conf", "/etc/untouched.conf"];
    let plan = SnapshotPlan { files: files.map(String::from).into(), ..Default::default() };
    let m = create(&root, &plan).unwrap();
    for f in ["a.conf", "b.conf", "new.conf"] {
        fs::write(root.join("etc").join(f), "bindle\n").unwrap();
        record_written(&root, &m.id, &format!("/etc/{f}")).unwrap();
    }
    // Since then: b.conf edited, new.conf deleted, gone.conf (never written)
    // created by something else, untouched.conf edited.
    fs::write(root.join("etc/b.conf"), "mine\n").unwrap();
    fs::remove_file(root.join("etc/new.conf")).unwrap();
    fs::write(root.join("etc/gone.conf"), "other\n").unwrap();
    fs::write(root.join("etc/untouched.conf"), "edited\n").unwrap();
    let m = load(&root, &m.id).unwrap();
    assert!(m.tracks_writes && m.files[0].written.as_deref().is_some_and(|w| w.len() == 64));

    let r = restore_files(&root, &m, false);
    assert!(r.errors.is_empty(), "{r:?}");
    assert_eq!(fs::read_to_string(root.join("etc/a.conf")).unwrap(), "a\n", "unchanged since: restored");
    let kept: Vec<&str> = r.kept.iter().map(|k| k.split(':').next().unwrap()).collect();
    assert_eq!(kept, ["/etc/b.conf", "/etc/new.conf", "/etc/gone.conf", "/etc/untouched.conf"]);
    assert!(r.kept[0].ends_with(KEPT));
    assert_eq!(fs::read_to_string(root.join("etc/b.conf")).unwrap(), "mine\n");
    assert_eq!(fs::read_to_string(root.join("etc/gone.conf")).unwrap(), "other\n");

    // --force puts every original back.
    let r = restore_files(&root, &m, true);
    assert!(r.errors.is_empty() && r.kept.is_empty(), "{r:?}");
    assert_eq!(fs::read_to_string(root.join("etc/b.conf")).unwrap(), "b\n");
    assert_eq!(fs::read_to_string(root.join("etc/untouched.conf")).unwrap(), "u\n");
    assert!(!root.join("etc/new.conf").exists() && !root.join("etc/gone.conf").exists());
}

#[test]
fn snapshots_without_write_records_restore_as_before() {
    let root = crate::sys::test_dir("snapshot-old");
    fs::create_dir_all(root.join("etc")).unwrap();
    fs::write(root.join("etc/a.conf"), "a\n").unwrap();
    let plan = SnapshotPlan { files: vec!["/etc/a.conf".into()], ..Default::default() };
    let id = create(&root, &plan).unwrap().id;
    // A manifest as an older Bindle wrote it: no `tracks_writes`, no `written`.
    let path = host(&root, DIR).join(&id).join("manifest.json");
    let mut json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("tracks_writes");
    fs::write(&path, json.to_string()).unwrap();
    fs::write(root.join("etc/a.conf"), "edited later\n").unwrap();
    let m = load(&root, &id).unwrap();
    assert!(!m.tracks_writes);
    assert!(restore_files(&root, &m, false).kept.is_empty());
    assert_eq!(fs::read_to_string(root.join("etc/a.conf")).unwrap(), "a\n");
}

#[test]
fn restore_continues_past_errors_and_removes_created_dirs() {
    let root = crate::sys::test_dir("snapshot-errors");
    fs::create_dir_all(root.join("etc")).unwrap();
    fs::write(root.join("etc/a.conf"), "a\n").unwrap();
    fs::write(root.join("etc/b.conf"), "b\n").unwrap();
    let plan = SnapshotPlan {
        files: vec!["/etc/a.conf".into(), "/etc/b.conf".into(), "/etc/new.d/x/c.conf".into()],
        ..Default::default()
    };
    let m = create(&root, &plan).unwrap();
    assert_eq!(m.created_dirs, ["/etc/new.d/x", "/etc/new.d"]);
    fs::write(root.join("etc/b.conf"), "changed\n").unwrap();
    fs::create_dir_all(root.join("etc/new.d/x")).unwrap();
    fs::write(root.join("etc/new.d/x/c.conf"), "c\n").unwrap();
    // a.conf became a symlink: restoring it fails, the rest still happens.
    fs::remove_file(root.join("etc/a.conf")).unwrap();
    std::os::unix::fs::symlink("/dev/null", root.join("etc/a.conf")).unwrap();
    // Forced: these edits weren't recorded as the apply's own.
    let r = restore_files(&root, &m, true);
    assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
    assert!(r.errors[0].starts_with("/etc/a.conf"));
    assert_eq!(fs::read_to_string(root.join("etc/b.conf")).unwrap(), "b\n");
    assert!(!root.join("etc/new.d").exists());
}

#[test]
fn mount_points_the_apply_creates_go_again_unless_something_is_in_them() {
    let root = crate::sys::test_dir("snapshot-dirs");
    fs::create_dir_all(root.join("mnt/Kept")).unwrap();
    let plan =
        SnapshotPlan { dirs: vec!["/mnt/Data".into(), "/mnt/Games".into(), "/mnt/Kept".into()], ..Default::default() };
    let m = create(&root, &plan).unwrap();
    assert_eq!(m.created_dirs, ["/mnt/Data", "/mnt/Games"]);
    fs::create_dir_all(root.join("mnt/Data")).unwrap();
    fs::create_dir_all(root.join("mnt/Games")).unwrap();
    fs::write(root.join("mnt/Games/save"), "x").unwrap();
    restore_files(&root, &m, false);
    assert!(!root.join("mnt/Data").exists());
    assert!(root.join("mnt/Games/save").exists() && root.join("mnt/Kept").exists());
}

#[test]
fn ids_sort_and_sessions_chain_incomplete_snapshots() {
    let root = crate::sys::test_dir("snapshot-session");
    let plan = SnapshotPlan::default();
    let ids: Vec<String> = (0..12).map(|_| create(&root, &plan).unwrap().id).collect();
    let listed: Vec<String> = list(&root).into_iter().rev().map(|m| m.id).collect();
    assert_eq!(listed, ids, "creation order == sort order");
    // Run 1..10 finished, 11 failed, 12 is the resume.
    for id in &ids[..10] {
        mark_complete(&root, id).unwrap();
    }
    let session: Vec<String> = latest_session(&list(&root)).into_iter().map(|m| m.id).collect();
    assert_eq!(session, [ids[11].clone(), ids[10].clone()]);
    mark_complete(&root, &ids[11]).unwrap();
    assert_eq!(latest_session(&list(&root)).len(), 2, "a finished resume still undoes the failed run");
    // Undoing that session moves plain `undo` on to the run before it.
    mark_undone(&root, &ids[11]).unwrap();
    mark_undone(&root, &ids[10]).unwrap();
    let next: Vec<String> = latest_session(&list(&root)).into_iter().map(|m| m.id).collect();
    assert_eq!(next, [ids[9].clone()]);
    for id in &ids[..10] {
        mark_undone(&root, id).unwrap();
    }
    assert!(latest_session(&list(&root)).is_empty());
}

#[test]
fn a_failed_apply_of_another_selection_is_not_chained() {
    let root = crate::sys::test_dir("snapshot-labels");
    let with = |label: &str| create(&root, &SnapshotPlan { label: label.into(), ..Default::default() }).unwrap().id;
    // Two failed tries of one selection, a failed try of another, then its resume.
    let ids = [with("a, b"), with("a, b"), with("a, c"), with("a, c")];
    mark_complete(&root, &ids[3]).unwrap();
    let session = |root: &Path| latest_session(&list(root)).into_iter().map(|m| m.id).collect::<Vec<_>>();
    assert_eq!(session(&root), [ids[3].clone(), ids[2].clone()]);
    mark_undone(&root, &ids[3]).unwrap();
    mark_undone(&root, &ids[2]).unwrap();
    assert_eq!(session(&root), [ids[1].clone(), ids[0].clone()], "the next undo takes the earlier tries");
}
