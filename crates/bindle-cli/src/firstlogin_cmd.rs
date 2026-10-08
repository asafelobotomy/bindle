//! `bindle first-login …` subcommands: dispatch, and the `run` flow that the
//! autostart entry, the user service and the console hook all end up in.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use bindle_core::{Catalog, Facts};

use crate::apply::{self, sys::is_root};
use crate::{FirstLoginCmd, firstlogin, notify, session, terminal, welcome};

/// Every subcommand except `run` (which needs hardware detection).
pub fn dispatch(action: FirstLoginCmd) -> Result<()> {
    let lines = match action {
        FirstLoginCmd::Install { root, no_tty_hook } => firstlogin::install(&root, !no_tty_hook)?,
        FirstLoginCmd::Remove { root, purge } => firstlogin::remove(&root, purge)?,
        FirstLoginCmd::Status { root } => firstlogin::status(&root),
        FirstLoginCmd::Snippet { url, sha256 } => vec![firstlogin::archinstall_snippet(&url, sha256.as_deref())?],
        FirstLoginCmd::Watch => return session::watch(),
        FirstLoginCmd::QuietWelcome { root } => quiet_welcome(&root)?,
        FirstLoginCmd::Dismiss => {
            firstlogin::mark_done()?;
            vec!["Bindle won't open or remind you at login again. Run `bindle` any time.".into()]
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
        let exe = std::env::current_exe()?.to_string_lossy().into_owned();
        let mut argv = vec![exe, "first-login".into(), "run".into(), "--in-terminal".into()];
        if let Some(t) = &token {
            argv.extend(["--lock".into(), t.clone()]);
        }
        return terminal::spawn(&argv).inspect_err(|_| release());
    }
    if seen && !in_terminal {
        // Console login after the first: a one-line reminder instead of the wizard.
        println!(
            "Bindle: this system isn't fully set up yet. Run `bindle` to continue, or `bindle first-login dismiss`."
        );
        return Ok(());
    }
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
    } else {
        println!("Bindle will remind you with a notification at your next login.");
    }
    Ok(())
}
