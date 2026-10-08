//! Small system primitives shared by the orchestrator and the helper.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

/// Map an absolute system path into `root` ("/" in production, a temp dir in
/// sandbox mode). Rejects relative paths and `..`.
pub fn host(root: &Path, path: &str) -> PathBuf {
    root.join(path.trim_start_matches('/'))
}

pub fn check_system_path(path: &str) -> Result<()> {
    let p = Path::new(path);
    if !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
        bail!("refusing unsafe path `{path}`");
    }
    Ok(())
}

pub fn is_root() -> bool {
    fs::metadata("/proc/self").is_ok_and(|m| m.uid() == 0)
}

/// Write via a temp file + rename so a crash never leaves a half-written file.
pub fn write_atomic(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    let dir = path.parent().context("path has no parent")?;
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let tmp = dir.join(format!(".{}.bindle-tmp", path.file_name().unwrap_or_default().to_string_lossy()));
    {
        let mut f = fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
    fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

fn civil(secs: u64) -> (i64, u32, u32, u64) {
    // Howard Hinnant's days-to-civil.
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d, secs % 86_400)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// `20261008-153012` (UTC), sortable; used for snapshot ids and log names.
pub fn now_id() -> String {
    let (y, m, d, s) = civil(now_secs());
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", s / 3600, s / 60 % 60, s % 60)
}

pub fn now_rfc3339() -> String {
    let (y, m, d, s) = civil(now_secs());
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, s / 60 % 60, s % 60)
}

pub fn shell_words(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if !a.is_empty() && a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c)) {
                a.clone()
            } else {
                format!("'{}'", a.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run a command, passing each stdout/stderr line to `on_line` as it arrives.
pub fn run_streaming(argv: &[String], on_line: &mut dyn FnMut(&str)) -> Result<()> {
    let (prog, args) = argv.split_first().context("empty command")?;
    let mut child = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start `{prog}`"))?;
    let (tx, rx) = mpsc::channel::<String>();
    let readers: Vec<_> = [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|stream| {
        let tx = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        })
    })
    .collect();
    drop(tx);
    for line in rx {
        on_line(&line);
    }
    readers.into_iter().for_each(|r| drop(r.join()));
    let status = child.wait()?;
    if !status.success() {
        bail!("`{}` failed ({status})", shell_words(argv));
    }
    Ok(())
}

#[cfg(test)]
pub fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bindle-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_paths() {
        assert_eq!(civil(0), (1970, 1, 1, 0));
        assert_eq!(civil(1_791_461_412).0, 2026);
        assert!(check_system_path("/etc/x").is_ok());
        assert!(check_system_path("/etc/../root/x").is_err());
        assert!(check_system_path("etc/x").is_err());
        assert_eq!(host(Path::new("/tmp/r"), "/etc/a"), PathBuf::from("/tmp/r/etc/a"));
        assert_eq!(shell_words(&["echo".into(), "a b".into()]), "echo 'a b'");
    }

    #[test]
    fn streams_output_and_reports_failure() {
        let mut lines = Vec::new();
        run_streaming(&["sh".into(), "-c".into(), "echo out; echo err >&2".into()], &mut |l| lines.push(l.to_string()))
            .unwrap();
        lines.sort();
        assert_eq!(lines, ["err", "out"]);
        assert!(run_streaming(&["false".into()], &mut |_| {}).is_err());
    }
}
