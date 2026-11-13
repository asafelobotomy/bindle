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
    fs::write(root.join("etc/pacman.conf"), "changed\n").unwrap();
    fs::write(root.join("etc/new.conf"), "new\n").unwrap();

    let loaded = &list(&root)[0];
    assert_eq!(loaded.id, m.id);
    assert!(restore_files(&root, loaded).errors.is_empty());
    assert_eq!(fs::read_to_string(root.join("etc/pacman.conf")).unwrap(), "original\n");
    assert!(!root.join("etc/new.conf").exists());
    assert!(load(&root, "../etc").is_err());

    // A package created the file after the snapshot: undo keeps that version.
    let m = create(&root, &plan).unwrap();
    fs::write(root.join("etc/new.conf"), "from package\n").unwrap();
    capture_late(&root, &m.id, "/etc/new.conf").unwrap();
    fs::write(root.join("etc/new.conf"), "bindle\n").unwrap();
    assert!(restore_files(&root, &load(&root, &m.id).unwrap()).errors.is_empty());
    assert_eq!(fs::read_to_string(root.join("etc/new.conf")).unwrap(), "from package\n");
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
    let r = restore_files(&root, &m);
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
    restore_files(&root, &m);
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
