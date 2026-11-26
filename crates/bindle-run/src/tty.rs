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

/// Keystrokes typed into the terminal while the password dialog is up (or
/// during the unattended run) must never reach the shell afterwards: a
/// password typed here by mistake would otherwise sit in the terminal's
/// input queue and appear at the next prompt, or run as a command and land
/// in the shell's history. While this lives the terminal on `fd` doesn't
/// echo; when it drops, whatever was typed is thrown away and the
/// terminal's settings are put back. Not a terminal: does nothing.
pub struct HiddenInput {
    fd: libc::c_int,
    saved: Option<libc::termios>,
}

impl HiddenInput {
    /// Stop echoing on this process's stdin.
    pub fn stdin() -> Self {
        Self::on(libc::STDIN_FILENO)
    }

    fn on(fd: libc::c_int) -> Self {
        // SAFETY: isatty/tcgetattr/tcsetattr on a file descriptor, with a zeroed termios we own.
        let saved = unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            (libc::isatty(fd) == 1 && libc::tcgetattr(fd, &mut t) == 0).then_some(t)
        };
        if let Some(t) = saved {
            discard(fd);
            let mut quiet = t;
            quiet.c_lflag &= !(libc::ECHO | libc::ECHONL);
            // SAFETY: as above; `quiet` is a copy of the terminal's own settings.
            unsafe { libc::tcsetattr(fd, libc::TCSANOW, &quiet) };
        }
        Self { fd, saved }
    }
}

impl Drop for HiddenInput {
    fn drop(&mut self) {
        if let Some(t) = self.saved {
            discard(self.fd);
            // SAFETY: puts back the settings tcgetattr returned for this descriptor.
            unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, &t) };
        }
    }
}

/// Throw away anything typed on this process's terminal that nobody read,
/// so it can't answer a later question or reach the shell when Bindle ends.
pub fn discard_typeahead() {
    discard(libc::STDIN_FILENO);
}

fn discard(fd: libc::c_int) {
    // SAFETY: isatty/tcflush only act on the descriptor; no memory is passed.
    unsafe {
        if libc::isatty(fd) == 1 {
            libc::tcflush(fd, libc::TCIFLUSH);
        }
    }
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

    /// A terminal pair: (the side a terminal emulator writes keystrokes to, the side Bindle reads).
    fn pty() -> (File, File) {
        use std::os::fd::FromRawFd;
        let (mut master, mut slave) = (0, 0);
        // SAFETY: openpty fills two descriptors we then own; null name/termios/winsize are allowed.
        let rc =
            unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), std::ptr::null()) };
        assert_eq!(rc, 0, "openpty");
        // SAFETY: both descriptors were just opened and are owned by nobody else.
        unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) }
    }

    fn lflag(f: &File) -> libc::tcflag_t {
        use std::os::fd::AsRawFd;
        // SAFETY: tcgetattr into a zeroed termios we own.
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            assert_eq!(libc::tcgetattr(f.as_raw_fd(), &mut t), 0);
            t.c_lflag
        }
    }

    /// Bytes waiting to be read on `f`, without blocking.
    fn pending(f: &File) -> usize {
        use std::os::fd::AsRawFd;
        let mut n: libc::c_int = 0;
        // SAFETY: FIONREAD writes one c_int.
        unsafe { libc::ioctl(f.as_raw_fd(), libc::FIONREAD, &mut n) };
        n as usize
    }

    #[test]
    fn a_password_typed_into_the_terminal_is_neither_shown_nor_left_for_the_shell() {
        use std::io::Write;
        use std::os::fd::AsRawFd;
        let (mut keys, term) = pty();
        let before = lflag(&term);
        assert!(before & libc::ECHO != 0, "a fresh terminal echoes");
        (&keys).write_all(b"typed before\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        {
            let _hidden = HiddenInput::on(term.as_raw_fd());
            assert_eq!(pending(&term), 0, "what was typed before the prompt is thrown away");
            assert_eq!(lflag(&term) & libc::ECHO, 0, "nothing typed now is shown");
            keys.write_all(b"hunter2\n").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            assert!(pending(&term) > 0, "the keystrokes reached the terminal");
        }
        assert_eq!(pending(&term), 0, "the password never reaches the shell");
        assert_eq!(lflag(&term), before, "the terminal's settings are put back");
        // Nothing was echoed back to the screen while hidden: the master side has only
        // the echo of what was typed before hiding.
        let mut screen = vec![0u8; 256];
        use std::io::Read;
        // SAFETY: O_NONBLOCK on our own descriptor so an empty read returns at once.
        unsafe { libc::fcntl(keys.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) };
        let n = keys.read(&mut screen).unwrap_or(0);
        let shown = String::from_utf8_lossy(&screen[..n]);
        assert!(!shown.contains("hunter2"), "echoed: {shown:?}");
    }

    #[test]
    fn typeahead_is_discarded_and_a_non_terminal_is_left_alone() {
        use std::io::Write;
        use std::os::fd::AsRawFd;
        let (keys, term) = pty();
        (&keys).write_all(b"y\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(pending(&term) > 0);
        discard(term.as_raw_fd());
        assert_eq!(pending(&term), 0, "a stale `y` can't answer pacman's next question");
        let file = File::open("/etc/passwd").unwrap();
        drop(HiddenInput::on(file.as_raw_fd()));
        discard(file.as_raw_fd());
    }
}
