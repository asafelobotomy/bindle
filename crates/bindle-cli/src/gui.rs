//! Opening the desktop window (`bindle-gui`) instead of a terminal, when this
//! is a graphical session and the window can start.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

/// Where Kirigami's QML module lives on Arch; without it the window can't load.
const KIRIGAMI: &str = "/usr/lib/qt6/qml/org/kde/kirigami";

/// `bindle-gui`'s exit codes (see `crates/bindle-gui/src/main.rs`).
pub const EXIT_APPLIED: i32 = 0;
pub const EXIT_CLOSED: i32 = 3;

#[derive(Debug, PartialEq)]
pub enum Ended {
    /// The plan was applied.
    Applied,
    /// Closed without applying.
    Closed,
    /// The window couldn't start (no Qt, no display…): use the terminal.
    Failed,
}

/// `bindle-gui` installed next to this program, a display to show it on, and
/// Kirigami installed.
pub fn available() -> Option<PathBuf> {
    let display = std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some();
    let gui = std::env::current_exe().ok()?.with_file_name("bindle-gui");
    usable(&gui, display, Path::new(KIRIGAMI)).then_some(gui)
}

fn usable(gui: &Path, display: bool, kirigami: &Path) -> bool {
    display && gui.is_file() && kirigami.is_dir()
}

/// Run the window and wait for it.
pub fn run(gui: &Path, first_login: bool) -> Ended {
    let mut cmd = Command::new(gui);
    if first_login {
        cmd.arg("--first-login");
    }
    wait(cmd)
}

fn wait(mut cmd: Command) -> Ended {
    match cmd.status().ok().and_then(|s| s.code()) {
        Some(EXIT_APPLIED) => Ended::Applied,
        Some(EXIT_CLOSED) => Ended::Closed,
        _ => Ended::Failed,
    }
}

/// The app-menu entry: the window if it can start, else the terminal wizard.
pub fn open() -> Result<()> {
    if let Some(gui) = available()
        && run(&gui, false) != Ended::Failed
    {
        return Ok(());
    }
    let exe = std::env::current_exe()?.to_string_lossy().into_owned();
    crate::terminal::spawn(&[exe])
}

/// `bindle open` in a new session, not waited for: it outlives the caller and
/// anything that ends the caller's process group.
pub fn open_detached() -> Result<()> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("open")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: setsid is async-signal-safe and touches no memory of this process.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_a_display_the_binary_and_kirigami() {
        let dir = crate::apply::sys::test_dir("gui");
        let gui = dir.join("bindle-gui");
        std::fs::write(&gui, "").unwrap();
        assert!(usable(&gui, true, &dir));
        assert!(!usable(&gui, false, &dir));
        assert!(!usable(&dir.join("missing"), true, &dir));
        assert!(!usable(&gui, true, &dir.join("no-kirigami")));
    }

    #[test]
    fn exit_codes_say_how_the_window_ended() {
        // `sh -c`, not a script written here and exec'd: another test's fork
        // could still hold its write handle (ETXTBSY, "Text file busy").
        let sh = |code: i32| {
            let mut cmd = Command::new("/bin/sh");
            cmd.args(["-c", &format!("exit {code}")]);
            wait(cmd)
        };
        assert_eq!(sh(EXIT_APPLIED), Ended::Applied);
        assert_eq!(sh(EXIT_CLOSED), Ended::Closed);
        assert_eq!(sh(4), Ended::Failed);
        assert_eq!(run(Path::new("/nonexistent/bindle-gui"), true), Ended::Failed);
    }
}
