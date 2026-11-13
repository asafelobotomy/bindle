//! Pre-apply snapshots under /var/lib/bindle/snapshots/<id>/.
//! manifest.json is world-readable (paths and package names only); copies of
//! the original files live in a root-only `files/` directory.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::protocol::SnapshotPlan;
use crate::sys::{check_system_path, create_dirs, host, now_id, now_rfc3339, write_atomic, write_atomic_owned};

pub const DIR: &str = "/var/lib/bindle/snapshots";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub existed: bool,
    pub mode: Option<u32>,
    #[serde(default)]
    pub owner: Option<(u32, u32)>,
    pub stored: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub created: String,
    pub label: String,
    pub user: String,
    pub files: Vec<FileEntry>,
    pub units: Vec<String>,
    pub packages: Vec<String>,
    pub aur: Vec<String>,
    /// Changes undo can't reverse (commands, group membership); shown to the user.
    #[serde(default)]
    pub manual: Vec<String>,
    /// Directories that didn't exist and were created for new files (deepest first).
    #[serde(default)]
    pub created_dirs: Vec<String>,
    /// The apply that took this snapshot finished. A failed apply that was
    /// re-run with the same selection leaves an incomplete snapshot that
    /// `undo` reverts together with the re-run's. Snapshots from before this field count as complete.
    #[serde(default = "complete_default")]
    pub complete: bool,
    /// `bindle undo` restored it; plain `undo` moves on to the run before.
    #[serde(default)]
    pub undone: bool,
}

fn complete_default() -> bool {
    true
}

/// What a restore did, and what it could not do.
#[derive(Debug, Default)]
pub struct RestoreReport {
    pub done: Vec<String>,
    pub errors: Vec<String>,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_digit() || c == '-')
}

fn snapshot_dir(root: &Path, id: &str) -> PathBuf {
    host(root, DIR).join(id)
}

pub fn create(root: &Path, plan: &SnapshotPlan) -> Result<Manifest> {
    let base = host(root, DIR);
    create_dirs(&base)?;
    // create_dir is the claim: two applies can never share a directory.
    // Same-second ids get a zero-padded suffix so they sort in order.
    let stamp = now_id();
    let mut n = 1;
    let (id, dir) = loop {
        let id = if n == 1 { stamp.clone() } else { format!("{stamp}-{n:02}") };
        let dir = base.join(&id);
        match fs::create_dir(&dir) {
            Ok(()) => break (id, dir),
            Err(e) if e.kind() == ErrorKind::AlreadyExists && n < 99 => n += 1,
            Err(e) => return Err(e).with_context(|| format!("create snapshot {}", dir.display())),
        }
    };
    let result = fill(root, plan, &id, &dir);
    if result.is_err() {
        // Nothing was changed; don't leave a half-written snapshot behind.
        let _ = fs::remove_dir_all(&dir);
    }
    result
}

fn fill(root: &Path, plan: &SnapshotPlan, id: &str, dir: &Path) -> Result<Manifest> {
    let id = id.to_string();
    let files_dir = dir.join("files");
    fs::create_dir(&files_dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755))?;
    fs::set_permissions(&files_dir, fs::Permissions::from_mode(0o700))?;

    let mut files = Vec::new();
    let mut created_dirs = Vec::new();
    for (i, path) in plan.files.iter().enumerate() {
        files.push(capture(root, &files_dir, path, &i.to_string())?);
        for d in missing_dirs(root, path) {
            if !created_dirs.contains(&d) {
                created_dirs.push(d);
            }
        }
    }
    for dir in &plan.dirs {
        // `missing_dirs` lists a path's missing ancestors: ask about a file inside.
        for d in missing_dirs(root, &format!("{dir}/x")) {
            if !created_dirs.contains(&d) {
                created_dirs.push(d);
            }
        }
    }
    created_dirs.sort_by_key(|d: &String| std::cmp::Reverse(d.matches('/').count()));
    let manifest = Manifest {
        id,
        created: now_rfc3339(),
        label: plan.label.clone(),
        user: plan.user.clone(),
        files,
        units: plan.units.clone(),
        packages: plan.packages.clone(),
        aur: plan.aur.clone(),
        manual: plan.manual.clone(),
        created_dirs,
        complete: false,
        undone: false,
    };
    write_atomic(&dir.join("manifest.json"), serde_json::to_string_pretty(&manifest)?.as_bytes(), 0o644)?;
    Ok(manifest)
}

/// Ancestors of `path` that don't exist yet (so apply will create them).
fn missing_dirs(root: &Path, path: &str) -> Vec<String> {
    Path::new(path)
        .ancestors()
        .skip(1)
        .take_while(|d| *d != Path::new("/") && fs::symlink_metadata(host(root, &d.to_string_lossy())).is_err())
        .map(|d| d.to_string_lossy().into_owned())
        .collect()
}

/// Record `path` as it is now: a copy if it exists, else "didn't exist".
fn capture(root: &Path, files_dir: &Path, path: &str, stored: &str) -> Result<FileEntry> {
    let src = host(root, path);
    match fs::symlink_metadata(&src) {
        Ok(meta) if meta.is_file() => {
            fs::copy(&src, files_dir.join(stored)).with_context(|| format!("snapshot {path}"))?;
            Ok(FileEntry {
                path: path.to_string(),
                existed: true,
                mode: Some(meta.permissions().mode() & 0o7777),
                owner: Some((meta.uid(), meta.gid())),
                stored: Some(stored.to_string()),
            })
        }
        Ok(_) => bail!("{path} exists but is not a regular file"),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            Ok(FileEntry { path: path.to_string(), existed: false, mode: None, owner: None, stored: None })
        }
        Err(e) => Err(e).with_context(|| format!("snapshot {path}")),
    }
}

/// Just before a file is written: if the snapshot saw it missing but it exists
/// now (a package installed earlier in this apply ships it), store that copy,
/// so undo restores the package's version instead of deleting it.
pub fn capture_late(root: &Path, id: &str, path: &str) -> Result<()> {
    let mut m = load(root, id)?;
    let late: Vec<usize> = (0..m.files.len()).filter(|&i| m.files[i].path == path && !m.files[i].existed).collect();
    if late.is_empty() || !host(root, path).exists() {
        return Ok(());
    }
    let dir = snapshot_dir(root, id);
    for i in late {
        m.files[i] = capture(root, &dir.join("files"), path, &i.to_string())?;
    }
    save(root, &m)
}

fn save(root: &Path, m: &Manifest) -> Result<()> {
    let path = snapshot_dir(root, &m.id).join("manifest.json");
    write_atomic(&path, serde_json::to_string_pretty(m)?.as_bytes(), 0o644)
}

/// The apply that took snapshot `id` finished.
pub fn mark_complete(root: &Path, id: &str) -> Result<()> {
    let mut m = load(root, id)?;
    m.complete = true;
    save(root, &m)
}

pub fn mark_undone(root: &Path, id: &str) -> Result<()> {
    let mut m = load(root, id)?;
    m.undone = true;
    save(root, &m)
}

/// What plain `bindle undo` reverts, newest first: the latest snapshot not
/// yet undone, plus the incomplete ones right before it that it resumed:
/// failed applies of the same selection (the label is its module list). A
/// failed apply of a different selection is a run of its own, undone by the
/// next plain `undo`. `snaps` is newest first, as `list` returns it.
pub fn latest_session(snaps: &[Manifest]) -> Vec<Manifest> {
    let mut live = snaps.iter().filter(|m| !m.undone);
    let Some(latest) = live.next() else { return Vec::new() };
    let mut out = vec![latest.clone()];
    out.extend(live.take_while(|m| !m.complete && m.label == latest.label).cloned());
    out
}

pub fn load(root: &Path, id: &str) -> Result<Manifest> {
    if !valid_id(id) {
        bail!("invalid snapshot id `{id}`");
    }
    let text = fs::read_to_string(snapshot_dir(root, id).join("manifest.json"))
        .with_context(|| format!("snapshot `{id}` not found"))?;
    Ok(serde_json::from_str(&text)?)
}

/// Newest first.
pub fn list(root: &Path) -> Vec<Manifest> {
    let mut out: Vec<Manifest> = fs::read_dir(host(root, DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| load(root, &e.file_name().to_string_lossy()).ok())
        .collect();
    out.sort_by(|a, b| b.id.cmp(&a.id));
    out
}

/// Restore files exactly as they were (removing ones that didn't exist, then
/// the directories created for them). One failure doesn't stop the rest.
pub fn restore_files(root: &Path, m: &Manifest) -> RestoreReport {
    let dir = snapshot_dir(root, &m.id).join("files");
    let mut r = RestoreReport::default();
    for f in &m.files {
        match restore_one(root, &dir, f) {
            Ok(Some(line)) => r.done.push(line),
            Ok(None) => {}
            Err(e) => r.errors.push(format!("{}: {e:#}", f.path)),
        }
    }
    for d in &m.created_dirs {
        // Only empty directories go; anything else put there since stays.
        if check_system_path(d).is_ok() && fs::remove_dir(host(root, d)).is_ok() {
            r.done.push(format!("removed directory {d}"));
        }
    }
    r
}

fn restore_one(root: &Path, dir: &Path, f: &FileEntry) -> Result<Option<String>> {
    check_system_path(&f.path)?;
    let target = host(root, &f.path);
    match (&f.stored, f.existed) {
        (Some(stored), true) => {
            if stored.is_empty() || !stored.chars().all(|c| c.is_ascii_digit()) {
                bail!("bad stored name `{stored}` in manifest");
            }
            let data = fs::read(dir.join(stored)).context("read stored copy")?;
            write_atomic_owned(&target, &data, f.mode.unwrap_or(0o644), f.owner)?;
            Ok(Some(format!("restored {}", f.path)))
        }
        _ if fs::symlink_metadata(&target).is_ok() => {
            fs::remove_file(&target).context("remove")?;
            Ok(Some(format!("removed {}", f.path)))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
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
        let plan = SnapshotPlan {
            dirs: vec!["/mnt/Data".into(), "/mnt/Games".into(), "/mnt/Kept".into()],
            ..Default::default()
        };
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
}
