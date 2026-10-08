//! Pre-apply snapshots under /var/lib/bindle/snapshots/<id>/.
//! manifest.json is world-readable (paths and package names only); copies of
//! the original files live in a root-only `files/` directory.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::protocol::SnapshotPlan;
use super::sys::{create_dirs, host, now_id, now_rfc3339, write_atomic, write_atomic_owned};

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
    let mut id = now_id();
    let mut n = 1;
    while base.join(&id).exists() {
        n += 1;
        id = format!("{}-{n}", now_id());
    }
    let dir = base.join(&id);
    let files_dir = dir.join("files");
    fs::create_dir_all(&files_dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))?;
    fs::set_permissions(&files_dir, fs::Permissions::from_mode(0o700))?;

    let mut files = Vec::new();
    for (i, path) in plan.files.iter().enumerate() {
        files.push(capture(root, &files_dir, path, &i.to_string())?);
    }
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
    };
    write_atomic(&dir.join("manifest.json"), serde_json::to_string_pretty(&manifest)?.as_bytes(), 0o644)?;
    Ok(manifest)
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
    let Some(i) = m.files.iter().position(|f| f.path == path && !f.existed) else {
        return Ok(());
    };
    if !host(root, path).exists() {
        return Ok(());
    }
    let dir = snapshot_dir(root, id);
    m.files[i] = capture(root, &dir.join("files"), path, &i.to_string())?;
    write_atomic(&dir.join("manifest.json"), serde_json::to_string_pretty(&m)?.as_bytes(), 0o644)
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

/// Restore files exactly as they were (removing ones that didn't exist).
pub fn restore_files(root: &Path, m: &Manifest) -> Result<Vec<String>> {
    let dir = snapshot_dir(root, &m.id).join("files");
    let mut done = Vec::new();
    for f in &m.files {
        let target = host(root, &f.path);
        match (&f.stored, f.existed) {
            (Some(stored), true) => {
                let data = fs::read(dir.join(stored)).with_context(|| format!("read stored copy of {}", f.path))?;
                write_atomic_owned(&target, &data, f.mode.unwrap_or(0o644), f.owner)?;
                done.push(format!("restored {}", f.path));
            }
            _ => {
                if fs::symlink_metadata(&target).is_ok() {
                    fs::remove_file(&target).with_context(|| format!("remove {}", f.path))?;
                    done.push(format!("removed {}", f.path));
                }
            }
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_and_restore_round_trip() {
        let root = super::super::sys::test_dir("snapshot");
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
        restore_files(&root, loaded).unwrap();
        assert_eq!(fs::read_to_string(root.join("etc/pacman.conf")).unwrap(), "original\n");
        assert!(!root.join("etc/new.conf").exists());
        assert!(load(&root, "../etc").is_err());

        // A package created the file after the snapshot: undo keeps that version.
        let m = create(&root, &plan).unwrap();
        fs::write(root.join("etc/new.conf"), "from package\n").unwrap();
        capture_late(&root, &m.id, "/etc/new.conf").unwrap();
        fs::write(root.join("etc/new.conf"), "bindle\n").unwrap();
        restore_files(&root, &load(&root, &m.id).unwrap()).unwrap();
        assert_eq!(fs::read_to_string(root.join("etc/new.conf")).unwrap(), "from package\n");
    }
}
