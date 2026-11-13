//! Running a root command on the user's own terminal, so it can ask its
//! questions (pacman's "Replace X with Y?", "Remove conflicting package?").
//! The helper's stdin and stdout carry the protocol; the command instead
//! gets the terminal the user started Bindle from, by path. Only a terminal
//! device owned by the user who authenticated is accepted.

use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::sys::shell_words;

/// The terminal on this process's stdin, as a path the helper can open.
pub fn own_tty() -> Option<String> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return None;
    }
    let path = fs::read_link("/proc/self/fd/0").ok()?.to_string_lossy().into_owned();
    valid_path(&path).then_some(path)
}

/// `/dev/pts/N` or `/dev/ttyN`, nothing else.
pub fn valid_path(path: &str) -> bool {
    let num = |s: &str| !s.is_empty() && s.len() <= 6 && s.chars().all(|c| c.is_ascii_digit());
    path.strip_prefix("/dev/pts/").is_some_and(num) || path.strip_prefix("/dev/tty").is_some_and(num)
}

/// Who started the helper: pkexec and sudo both say so.
pub fn caller_uid() -> Option<u32> {
    ["PKEXEC_UID", "SUDO_UID"].iter().find_map(|v| std::env::var(v).ok()?.parse().ok())
}

/// Open `path` for the command, after checking it is a terminal owned by `uid`.
fn open(path: &str, uid: u32) -> Result<File> {
    if !valid_path(path) {
        bail!("refusing terminal `{path}`");
    }
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY | libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("open {path}"))?;
    let m = f.metadata()?;
    if !m.file_type().is_char_device() || m.uid() != uid {
        bail!("{path} is not your terminal");
    }
    Ok(f)
}

/// The helper's side of `Request::Interactive`; a sandbox only records it.
pub fn serve(argv: &[String], path: &str, fake: bool, out: &mut dyn FnMut(&str)) -> Result<()> {
    if fake {
        out(&format!("would run on {path}: {}", shell_words(argv)));
        return Ok(());
    }
    run(argv, path, caller_uid().context("don't know who started the helper")?)
}

/// Run `argv` with the terminal at `path` as its stdin, stdout and stderr.
pub fn run(argv: &[String], path: &str, uid: u32) -> Result<()> {
    let tty = open(path, uid)?;
    let (prog, args) = argv.split_first().context("empty command")?;
    let status = Command::new(prog)
        .args(args)
        .stdin(Stdio::from(tty.try_clone()?))
        .stdout(Stdio::from(tty.try_clone()?))
        .stderr(Stdio::from(tty))
        .status()
        .with_context(|| format!("start `{prog}`"))?;
    if !status.success() {
        bail!("`{}` failed ({status})", shell_words(argv));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_terminal_paths() {
        assert!(valid_path("/dev/pts/3") && valid_path("/dev/tty2"));
        for bad in ["/dev/pts/", "/dev/pts/../sda", "/dev/tty", "/dev/ttyS0", "/etc/shadow", "/dev/pts/3/x"] {
            assert!(!valid_path(bad), "{bad}");
        }
    }

    #[test]
    fn a_sandbox_only_records() {
        let mut said = vec![];
        serve(&["pacman".into(), "-Syu".into()], "/dev/pts/7", true, &mut |l| said.push(l.to_string())).unwrap();
        assert_eq!(said, ["would run on /dev/pts/7: pacman -Syu"]);
    }

    #[test]
    fn a_file_that_isnt_a_terminal_is_refused() {
        assert!(open("/etc/passwd", 0).is_err());
        assert!(run(&["true".into()], "/dev/pts/999999", 0).is_err());
    }
}
