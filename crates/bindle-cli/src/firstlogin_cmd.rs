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
    /// Write again the files `install` put next to Bindle (uQ Settings' page, bar icon and
    /// problem codes, launcher, icons), without touching the login hooks (root). bindleup runs
    /// it after Bindle updated itself.
    Refresh {
        #[arg(long, default_value = "/")]
        root: PathBuf,
    },
    /// Remove the autostart entries (root); `--purge` also removes the binary.
    Remove {
        #[arg(long, default_value = "/")]
        root: PathBuf,
        /// Also remove the binary, launcher and icon.
        #[arg(long)]
        purge: bool,
        /// Only once every regular user has finished first-login setup.
        #[arg(long)]
        if_all_done: bool,
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
        FirstLoginCmd::Refresh { root } => firstlogin::refresh(&root)?,
        FirstLoginCmd::Remove { root, purge, if_all_done } => {
            let left = if if_all_done { firstlogin::unfinished(&root) } else { vec![] };
            if left.is_empty() {
                firstlogin::remove(&root, purge)?
            } else {
                let homes: Vec<String> = left.iter().map(|h| h.display().to_string()).collect();
                vec![format!("Kept: first-login setup isn't finished for {}.", homes.join(", "))]
            }
        }
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
/// to open for, except users created later: offer to remove them. Other
/// users' homes are private, so with others here only root can tell.
fn offer_remove() {
    let root = Path::new("/");
    if !firstlogin::hooks_installed(root) {
        return;
    }
    let exe = if Path::new(firstlogin::BIN).exists() { PathBuf::from(firstlogin::BIN) } else { "bindle".into() };
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if !home.is_some_and(|h| firstlogin::only_user(root, &h)) {
        println!("Once every user here has finished, remove the login hooks with:");
        println!("  pkexec {} first-login remove --if-all-done", exe.display());
        return;
    }
    println!("\nEvery user here has finished first-login setup. The login hooks stay installed");
    println!("(and new users still get the wizard) until they are removed.");
    if std::io::stdin().is_terminal() && apply::confirm("Remove them now? (asks for your password)", false) {
        match std::process::Command::new("pkexec").arg(&exe).args(["first-login", "remove"]).status() {
            Ok(s) if s.success() => return,
            Ok(_) | Err(_) => eprintln!("Couldn't remove them."),
        }
    }
    println!("Remove them any time with: pkexec {} first-login remove", exe.display());
}

#[cfg(test)]
mod tests;
