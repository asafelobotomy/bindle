//! Pre-apply snapshots under /var/lib/bindle/snapshots/<id>/.
//! manifest.json is world-readable (paths and package names only); copies of
//! the original files live in a root-only `files/` directory.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use bindle_core::plan::content::Content;

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
    /// Packages from Bindle's pinned releases (also in `packages`).
    #[serde(default)]
    pub releases: Vec<String>,
    /// Changes undo can't reverse (commands, group membership); shown to the user.
    #[serde(default)]
    pub manual: Vec<String>,
    /// Directories that didn't exist and were created for new files (deepest first).
    #[serde(default)]
    pub created_dirs: Vec<String>,
    /// The apply that took this snapshot finished. A failed apply that was
    /// re-run with the same selection leaves an incomplete snapshot that
    /// `undo` reverts together with the re-run's. Snapshots from before this field count as complete.
    #[serde(default = "crate::sys::yes")]
    pub complete: bool,
    /// `bindle undo` restored it; plain `undo` moves on to the run before.
    #[serde(default)]
    pub undone: bool,
    /// The Bindle that made this apply (empty before it was recorded).
    #[serde(default)]
    pub version: String,
    /// What each module of the apply puts on the system ("What's new"
    /// compares a newer Bindle's catalog with this).
    #[serde(default)]
    pub record: BTreeMap<String, Content>,
    /// Modules this apply retired: earlier records of them no longer count.
    #[serde(default)]
    pub forget: Vec<String>,
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
        releases: plan.releases.clone(),
        manual: plan.manual.clone(),
        created_dirs,
        complete: false,
        undone: false,
        version: plan.version.clone(),
        record: plan.record.clone(),
        forget: plan.forget.clone(),
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
#[path = "snapshot_tests.rs"]
mod tests;
