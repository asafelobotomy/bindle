//! `bindle first-login …` subcommands: dispatch, and the `run` flow that the
//! autostart entry, the user service and the console hook all end up in.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use bindle_core::{Catalog, Facts};
use clap::Subcommand;

use crate::apply::{self, sys::is_root};
use crate::{firstlogin, gui, notify, session, terminal, uq_handoff, welcome};

#[derive(Subcommand)]
pub enum FirstLoginCmd {
    /// Install bindle system-wide with autostart (root). `--root /mnt` from the live ISO.
    Install {
        #[arg(long, default_value = "/")]
        root: PathBuf,
        /// Skip the console-login hook (/etc/profile.d).
        #[arg(long)]
        no_tty_hook: bool,
    },
    /// Remove the autostart entries (root); `--purge` also removes the binary.
    Remove {
        #[arg(long, default_value = "/")]
        root: PathBuf,
        /// Also remove the binary, launcher and icon.
        #[arg(long)]
        purge: bool,
    },
    /// Show what is installed and whether this user has seen the wizard.
    Status {
        #[arg(long, default_value = "/")]
        root: PathBuf,
    },
    /// Run the first-login wizard (used by the autostart entry).
    Run {
        #[arg(long, hide = true)]
        in_terminal: bool,
        /// Launch-lock token to release (passed from the launching process).
        #[arg(long, hide = true)]
        lock: Option<String>,
    },
    /// Turn off desktop welcome apps (GNOME tour, KDE Welcome Center, Hyprland
    /// welcome) so Bindle is the first-login welcome. As root: system-wide (and
    /// users' homes with --root, used by `bindle iso`); as a user: your own
    /// Hyprland config.
    QuietWelcome {
        #[arg(long, default_value = "/")]
        root: PathBuf,
    },
    /// Stop first-login reminders for this user.
    Dismiss,
    /// Wait for a graphical session, then open the wizard (systemd user service).
    #[command(hide = true)]
    Watch,
    /// Print a custom_commands snippet for an archinstall config file.
    Snippet {
        /// Download URL of the bindle binary.
        #[arg(long)]
        url: String,
        /// Expected SHA-256 of the binary; the command verifies it before running.
        #[arg(long, value_name = "HEX")]
        sha256: Option<String>,
    },
}

/// Every subcommand except `run` (which needs hardware detection).
pub fn dispatch(action: FirstLoginCmd) -> Result<()> {
    let lines = match action {
        FirstLoginCmd::Install { root, no_tty_hook } => firstlogin::install(&root, !no_tty_hook)?,
        FirstLoginCmd::Remove { root, purge } => firstlogin::remove(&root, purge)?,
        FirstLoginCmd::Status { root } => firstlogin::status(&root),
        FirstLoginCmd::Snippet { url, sha256 } => {
            if sha256.is_none() {
                eprintln!(
                    "warning: without --sha256 the install runs whatever the URL serves; \
                     pass the binary's SHA-256 (from the release's signed SHA256SUMS)"
                );
            }
            vec![firstlogin::archinstall_snippet(&url, sha256.as_deref())?]
        }
        FirstLoginCmd::Watch => return session::watch(),
        FirstLoginCmd::QuietWelcome { root } => quiet_welcome(&root)?,
        FirstLoginCmd::Dismiss => {
            firstlogin::mark_done()?;
            println!("Bindle won't open or remind you at login again. Run `bindle` any time.");
            offer_remove();
            vec![]
        }
        FirstLoginCmd::Run { .. } => unreachable!("needs detection"),
    };
    lines.iter().for_each(|l| println!("{l}"));
    Ok(())
}

/// As root: system-wide defaults (plus homes under `--root`). As a user on
/// the running system: only that user's own Hyprland config, unprivileged.
fn quiet_welcome(root: &Path) -> Result<Vec<String>> {
    if root != Path::new("/") || is_root() {
        return welcome::quiet(root);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).context("HOME is not set")?;
    let mut lines = welcome::quiet_own_home(&home)?;
    if !Path::new(welcome::MARKER).exists() {
        lines.push("System-wide (GNOME, KDE, new users): pkexec bindle first-login quiet-welcome".into());
    }
    Ok(lines)
}

pub fn run(catalog: &Catalog, facts: &Facts, in_terminal: bool, lock: Option<String>) -> Result<()> {
    // `--in-terminal`: the launcher left its hold on uQ's Welcome to this
    // process. Take it over before anything can return, so that every way out
    // finishes it: a return, an error, or the window closing (SIGHUP).
    let inherited = if in_terminal { hold_handoff() } else { None };
    if !in_terminal && session::regular_user_session() {
        // Steps the install from the ISO left for a running session.
        crate::install_target::finish_pending();
    }
    if firstlogin::is_done() || !session::regular_user_session() {
        return Ok(());
    }
    let seen = firstlogin::is_seen();
    if !std::io::stdin().is_terminal() {
        if in_terminal {
            bail!("no terminal attached");
        }
        // Autostart and the user service can both fire in one session.
        let Some(token) = session::claim_launch() else { return Ok(()) };
        let release = || session::release_launch(token.as_deref());
        // In a uQ session, hold uQ's Welcome back until the wizard ends. First
        // thing, before any wait: uQ's shell decides about Welcome about 20 s
        // after it starts. Ending early (reminder dismissed) hands off at once.
        let handoff = uq_handoff::Handoff::begin();
        if seen {
            // Later logins get a reminder, not the wizard. Without a notification
            // server, fall back to opening the wizard so the reminder isn't lost.
            match notify::remind() {
                Ok(notify::Choice::Open) | Err(_) => {}
                Ok(notify::Choice::Dismiss) => {
                    release();
                    return firstlogin::mark_done();
                }
                Ok(notify::Choice::Ignored) => {
                    release();
                    return Ok(());
                }
            }
        } else {
            // Open after the desktop's own welcome app, so Bindle ends up on top.
            welcome::wait_for_welcome_apps();
        }
        // The desktop window when it can start; this process holds the
        // launch lock while it's open.
        if let Some(window) = gui::available() {
            let ended = gui::run(&window, true);
            if ended != gui::Ended::Failed {
                // The window failing to start falls through to the terminal,
                // which keeps holding the hand-off.
                if let Some(h) = handoff {
                    h.finish();
                }
                release();
                firstlogin::mark_seen()?;
                if ended == gui::Ended::Applied {
                    firstlogin::mark_done()?;
                }
                return Ok(());
            }
        }
        let exe = std::env::current_exe()?.to_string_lossy().into_owned();
        let mut argv = vec![exe, "first-login".into(), "run".into(), "--in-terminal".into()];
        if let Some(t) = &token {
            argv.extend(["--lock".into(), t.clone()]);
        }
        // The wizard in the terminal takes the hand-off over; if it can't
        // start, `handoff` drops here and Welcome opens.
        return terminal::spawn(&argv).inspect_err(|_| release()).map(|()| {
            if let Some(h) = handoff {
                h.disarm();
            }
        });
    }
    if seen && !in_terminal {
        // Console login after the first: a one-line reminder instead of the wizard.
        println!(
            "Bindle: this system isn't fully set up yet. Run `bindle` to continue, or `bindle first-login dismiss`."
        );
        return Ok(());
    }
    if in_terminal {
        // Hold the launch lock for as long as the wizard runs.
        session::adopt_launch(lock.as_deref());
    }
    // Held (taken over from the launcher above, or begun here) while the
    // wizard runs; dropped at the end of this function, opening uQ's Welcome.
    let _handoff = inherited.or_else(hold_handoff);
    let mut result = wizard_once(catalog, facts);
    // The window closes with this process, and a fresh desktop may have no
    // other way to open a terminal: offer a retry and a shell right here.
    while in_terminal && let Err(e) = &result {
        eprintln!("\nBindle stopped with an error: {e:#}");
        match after_error() {
            AfterError::Retry => result = wizard_once(catalog, facts),
            AfterError::Close => {
                eprintln!("It will open again at your next login; or run `bindle` any time.");
                break;
            }
        }
    }
    if in_terminal {
        session::release_launch(lock.as_deref());
        if result.is_ok() {
            apply::confirm("Press Enter to close.", true);
        }
    }
    result
}

enum AfterError {
    Retry,
    Close,
}

/// Ask what to do after a failed run: try again (finished steps are
/// skipped), open a shell to fix the cause (e.g. join Wi-Fi), or close.
fn after_error() -> AfterError {
    use std::io::{BufRead, Write};
    loop {
        eprintln!("\nEnter: try again (finished steps are skipped)");
        eprintln!(
            "s:     open a shell here to fix it, e.g. `nmcli device wifi connect <name> --ask`; `exit` comes back"
        );
        eprint!("q:     close  > ");
        let _ = std::io::stderr().flush();
        let mut answer = String::new();
        if std::io::stdin().lock().read_line(&mut answer).unwrap_or(0) == 0 {
            return AfterError::Close;
        }
        match answer.trim().to_lowercase().as_str() {
            "" | "r" | "y" => return AfterError::Retry,
            "q" | "n" => return AfterError::Close,
            "s" => {
                let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/bash".into());
                if let Err(e) = std::process::Command::new(&shell).status() {
                    eprintln!("couldn't start {shell}: {e}");
                }
            }
            _ => {}
        }
    }
}

/// Hold uQ's Welcome back while the wizard runs in this terminal, until this
/// process returns or a signal ends it.
fn hold_handoff() -> Option<uq_handoff::Held> {
    uq_handoff::Handoff::begin().map(uq_handoff::Handoff::finish_on_signals)
}

/// The wizard, then remember that it was shown. On error nothing is marked,
/// so the next login opens the wizard again rather than a reminder.
fn wizard_once(catalog: &Catalog, facts: &Facts) -> Result<()> {
    let finished = crate::wizard(catalog, facts, None)?;
    firstlogin::mark_seen()?;
    if finished || !apply::confirm("Remind me at my next login?", true) {
        firstlogin::mark_done()?;
        println!("Bindle won't open or remind you again. Run `bindle` any time.");
        offer_remove();
    } else {
        println!("Bindle will remind you with a notification at your next login.");
    }
    Ok(())
}

/// Once the last user is done, the system-wide login hooks have no one left
/// to open for, except users created later: offer to remove them.
fn offer_remove() {
    let root = Path::new("/");
    if !firstlogin::hooks_installed(root) || !firstlogin::all_users_done(root) {
        return;
    }
    println!("\nEvery user here has finished first-login setup. The login hooks stay installed");
    println!("(and new users still get the wizard) until they are removed.");
    let exe = if Path::new(firstlogin::BIN).exists() { PathBuf::from(firstlogin::BIN) } else { "bindle".into() };
    if std::io::stdin().is_terminal() && apply::confirm("Remove them now? (asks for your password)", false) {
        match std::process::Command::new("pkexec").arg(&exe).args(["first-login", "remove"]).status() {
            Ok(s) if s.success() => return,
            Ok(_) | Err(_) => eprintln!("Couldn't remove them."),
        }
    }
    println!("Remove them any time with: pkexec {} first-login remove", exe.display());
}

#[cfg(test)]
mod tests {
    //! Signals end a process, so these run the hand-off in a child: this test
    //! binary again, running one of the `child_*` tests (ignored otherwise).
    use super::*;
    use crate::uq_handoff::defer_path;
    use crate::uq_handoff::tests::{STUB_ENV, fake_uqctl, tempdir, wait_calls};
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// Set in the child: its scratch directory (the runtime dir, too).
    const CHILD_ENV: &str = "BINDLE_TEST_HANDOFF_DIR";

    fn spawn_child(test: &str, dir: &Path) -> std::process::Child {
        Command::new(std::env::current_exe().unwrap())
            .args([&format!("firstlogin_cmd::tests::{test}"), "--exact", "--ignored", "--test-threads=1"])
            .env(CHILD_ENV, dir)
            .env("XDG_RUNTIME_DIR", dir)
            .env("XDG_STATE_HOME", dir.join("state"))
            .env(STUB_ENV, fake_uqctl(dir))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    }

    fn wait_for(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !path.exists() {
            assert!(Instant::now() < deadline, "{} never appeared", path.display());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn closing_the_terminal_still_hands_off() {
        for signal in [libc::SIGHUP, libc::SIGTERM, libc::SIGINT] {
            let t = tempdir();
            let mut child = spawn_child("child_holds_the_handoff", t.path());
            wait_for(&t.path().join("ready"));
            assert!(defer_path(t.path()).is_file(), "held while the wizard runs");
            // SAFETY: kill(2) on our own child's pid; no memory is passed.
            assert_eq!(unsafe { libc::kill(child.id() as libc::pid_t, signal) }, 0);
            let status = child.wait().unwrap();
            assert_eq!(status.signal(), Some(signal), "ends as the signal would have: {status:?}");
            assert!(!defer_path(t.path()).exists(), "signal {signal}: defer file left behind");
            assert_eq!(wait_calls(t.path()).trim(), "welcome", "signal {signal}");
        }
    }

    #[test]
    fn returning_early_in_the_terminal_still_hands_off() {
        let t = tempdir();
        // The launcher's file, left for this process (it disarmed).
        std::fs::create_dir_all(defer_path(t.path()).parent().unwrap()).unwrap();
        std::fs::write(defer_path(t.path()), "bindle\n").unwrap();
        // Already done: `run` returns before the wizard.
        std::fs::create_dir_all(t.path().join("state/bindle")).unwrap();
        std::fs::write(t.path().join("state/bindle/first-login-done"), "").unwrap();
        let status = spawn_child("child_returns_early", t.path()).wait().unwrap();
        assert!(status.success(), "{status:?}");
        assert!(!defer_path(t.path()).exists());
        assert_eq!(wait_calls(t.path()).trim(), "welcome");
    }

    #[test]
    #[ignore = "run in a child by closing_the_terminal_still_hands_off"]
    fn child_holds_the_handoff() {
        let Some(dir) = std::env::var_os(CHILD_ENV).map(PathBuf::from) else { return };
        let _held = hold_handoff().expect("hand-off held");
        std::fs::write(dir.join("ready"), "").unwrap();
        // The wizard, until the signal ends this process.
        std::thread::sleep(Duration::from_secs(30));
        panic!("no signal arrived");
    }

    #[test]
    #[ignore = "run in a child by returning_early_in_the_terminal_still_hands_off"]
    fn child_returns_early() {
        if std::env::var_os(CHILD_ENV).is_none() {
            return;
        }
        run(&Catalog::arch().unwrap(), &Facts::default(), true, None).unwrap();
    }
}
