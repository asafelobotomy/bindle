//! `bindle first-login …` subcommands: dispatch, and the `run` flow that the
//! autostart entry, the user service and the console hook all end up in.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use bindle_core::{Catalog, Facts};

use crate::apply::{self, sys::is_root};
use crate::{FirstLoginCmd, firstlogin, gui, notify, session, terminal, uq_handoff, welcome};

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
        lines.push("System-wide (GNOME, KDE, new users): sudo bindle first-login quiet-welcome".into());
    }
    Ok(lines)
}

pub fn run(catalog: &Catalog, facts: &Facts, in_terminal: bool, lock: Option<String>) -> Result<()> {
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
        // In a uQ session, hold uQ's Welcome back until the wizard ends.
        let handoff = uq_handoff::Handoff::begin();
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
    // Held (or refreshed, after the hand-over from the launcher) while the
    // wizard runs; dropped at the end of this function, opening uQ's Welcome.
    let _handoff = uq_handoff::Handoff::begin();
    let result = wizard_once(catalog, facts);
    if in_terminal {
        // The window closes with this process: show any error and wait.
        if let Err(e) = &result {
            eprintln!("\nBindle stopped with an error: {e:#}");
            eprintln!("It will open again at your next login; or run `bindle` any time.");
        }
        session::release_launch(lock.as_deref());
        apply::confirm("Press Enter to close.", true);
    }
    result
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
    if std::io::stdin().is_terminal() && apply::confirm("Remove them now with sudo?", false) {
        match std::process::Command::new("sudo").arg(&exe).args(["first-login", "remove"]).status() {
            Ok(s) if s.success() => return,
            Ok(_) | Err(_) => eprintln!("Couldn't remove them."),
        }
    }
    println!("Remove them any time with: sudo {} first-login remove", exe.display());
}
