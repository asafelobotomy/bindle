//! `bindle install` (also `bindleinstall`): the whole install from the live
//! ISO in Bindle's own wizard. archinstall still does the disks, bootloader
//! and base system, unattended (`--silent`) from the answers given here; then
//! Bindle's plugin carries the Wi-Fi over and applies the chosen profile in
//! the new system, so the first boot lands in a finished desktop.

use std::fs;
use std::io::{BufRead, IsTerminal, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::ExitStatus;

use anyhow::{Context, Result, bail};
use bindle_core::install::target;
use bindle_core::{Catalog, Facts, desktops};

use crate::apply::sys::{is_root, write_atomic};
use crate::archinstall_version::{archinstall_version, version_note};
use crate::iso::{PLUGIN, WINDOW_RUNTIME, add_packages, is_live_iso, private_workdir};
use crate::livesys;
use crate::tui::install::guard;
use crate::tui::install::progress::{self, End};
use crate::{firstlogin, selection, tui};
use bindle_core::install::preset::TESTED_ARCHINSTALL;

pub struct InstallOpts {
    /// Write the config, credentials and plugin, print the command, run nothing.
    pub dry_run: bool,
    pub workdir: Option<PathBuf>,
}

pub fn run(catalog: &Catalog, facts: &Facts, opts: InstallOpts) -> Result<()> {
    if !opts.dry_run {
        is_live_iso()?;
        if !is_root() {
            bail!("run as root (the live ISO shell already is)");
        }
    }
    let version = archinstall_version();
    if version.is_none() && !opts.dry_run {
        bail!("archinstall not found on this ISO");
    }
    if !std::io::stdout().is_terminal() {
        bail!("`bindle install` needs a terminal");
    }
    let data = desktops::all()?;
    // Off the live ISO (a dry run) the wizard joins no Wi-Fi, opens no disk
    // editor and leaves the keyboard alone.
    let live = is_live_iso().is_ok();
    let Some(out) = tui::install::run(facts, catalog, &data, live)? else {
        println!("Cancelled. Nothing was changed.");
        return Ok(());
    };
    // The disk erased must be the one shown: same size and model now.
    if let Some(chosen) = &out.disk {
        match livesys::disk_now(&chosen.path) {
            Some(now) if now.same_as(chosen) => {}
            now => bail!(
                "{} changed since it was chosen (was {}, now {}). Nothing was changed; run the installer again.",
                chosen.path,
                chosen.label(),
                now.map_or("gone".into(), |d| d.label())
            ),
        }
    }

    let mut config = target::config(facts, &data, &out.choice, &out.answers);
    let window = std::env::current_exe().map(|e| e.with_file_name("bindle-gui").is_file()).unwrap_or(false);
    if window && out.choice.profile != "minimal" {
        add_packages(&mut config, WINDOW_RUNTIME);
    }
    let creds = target::creds(&out.answers);

    let work = match &opts.workdir {
        Some(w) => {
            crate::apply::sys::create_dirs(w)?;
            w.clone()
        }
        None => private_workdir()?,
    };
    let files = Files {
        config: work.join("bindle-archinstall.json"),
        creds: work.join("bindle-creds.json"),
        plugin: work.join("bindle_plugin.py"),
        selection: work.join("selection.toml"),
    };
    // Ctrl-C or SIGTERM from here on removes the passwords file first; a
    // hang-up (SSH dropped) no longer stops the install.
    let guard = guard::arm(vec![files.creds.clone(), files.config.clone(), files.selection.clone()]);
    write_atomic(&files.config, serde_json::to_string_pretty(&config)?.as_bytes(), 0o600)?;
    // A dry run leaves the files to look at, so no real password goes in them.
    let creds = if opts.dry_run { redacted(creds) } else { creds };
    write_atomic(&files.creds, serde_json::to_string(&creds)?.as_bytes(), 0o600)?;
    write_atomic(&files.plugin, PLUGIN.as_bytes(), 0o600)?;
    write_atomic(&files.selection, selection::render(&out.selection)?.as_bytes(), 0o600)?;

    let exe = std::env::current_exe()?;
    let mut cmd = Command::new("archinstall");
    cmd.arg("--config")
        .arg(&files.config)
        .arg("--creds")
        .arg(&files.creds)
        .arg("--plugin")
        .arg(&files.plugin)
        .arg("--silent")
        .env("BINDLE_BIN", &exe)
        .env("BINDLE_SELECTION", &files.selection)
        .env("BINDLE_SETUP_TARGET", "1")
        .env("BINDLE_USER", &out.answers.username);
    if opts.dry_run {
        println!(
            "Wrote {}, {} (passwords left out), {} and {}.",
            files.config.display(),
            files.creds.display(),
            files.plugin.display(),
            files.selection.display()
        );
        println!("Would run: {}", crate::apply::sys::shell_words(&argv(&cmd)));
        return Ok(());
    }
    if let Some(note) = version.as_deref().and_then(|v| version_note(v, TESTED_ARCHINSTALL)) {
        println!("{note}");
    }
    // One progress screen for the whole install; the plain terminal when the
    // screen can't be used (no `script` to give archinstall its own terminal).
    let title = format!("to {}", out.answers.disk);
    let user = out.answers.username.clone();
    let shown = progress::run(&cmd, title, Path::new(LOG), |status| outcome(status, &user));
    let done = match shown {
        Ok(Some(done)) => done,
        Ok(None) => {
            println!("\nInstalling to {}. This takes a while: Arch, then your desktop and apps.\n", out.answers.disk);
            // The hang-up of this terminal doesn't stop archinstall either.
            // SAFETY: signal is async-signal-safe and touches no memory of this process.
            unsafe {
                cmd.pre_exec(|| {
                    libc::signal(libc::SIGHUP, libc::SIG_IGN);
                    Ok(())
                });
            }
            let status = cmd.status();
            files.remove(opts.workdir.is_none().then_some(&work));
            guard.clear();
            let end = outcome(status.context("start archinstall")?, &user);
            if matches!(end, End::Failed(_)) {
                release_target(live, &out.answers);
            }
            return finish_plain(end);
        }
        Err(e) => {
            files.remove(opts.workdir.is_none().then_some(&work));
            return Err(e);
        }
    };
    files.remove(opts.workdir.is_none().then_some(&work));
    guard.clear();
    keep_log();
    match done.end {
        End::Failed(why) => {
            // Cleaned up first: with the terminal gone, printing can fail.
            release_target(live, &out.answers);
            done.tail.iter().for_each(|l| println!("{l}"));
            bail!("{why}\nEverything it printed: {LOG}");
        }
        End::Ready if done.restart => restart()?,
        End::Ready => println!("Your system is ready. Restart when you like: systemctl reboot"),
        End::Unfinished(why) => println!("{why}\nRestart when you like: systemctl reboot"),
    }
    Ok(())
}

/// Everything the install printed, on the ISO (in memory, gone at restart).
const LOG: &str = "/var/log/bindle-install.log";

/// Keep that log in the new system too.
fn keep_log() {
    let dir = Path::new("/mnt/var/log");
    if dir.is_dir() {
        let _ = fs::copy(LOG, dir.join("bindle-install.log"));
    }
}

/// The credentials with every password replaced (`--dry-run`).
fn redacted(mut creds: serde_json::Value) -> serde_json::Value {
    fn walk(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(map) => {
                for (k, x) in map.iter_mut() {
                    if k.starts_with('!') || k.contains("password") {
                        *x = "<left out: dry run>".into();
                    } else {
                        walk(x);
                    }
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(walk),
            _ => {}
        }
    }
    walk(&mut creds);
    creds
}

/// After a failed install on the live ISO: unmount /mnt and close what
/// archinstall opened on the target disks, so another try starts clean.
fn release_target(live: bool, a: &target::Answers) {
    if !live {
        return;
    }
    let disks: Vec<String> = a.disk.split(", ").map(String::from).collect();
    let done = livesys::release_target(&disks);
    if !done.is_empty() {
        println!("Cleaned up for another try: {}.", done.join(", "));
    }
}

struct Files {
    config: PathBuf,
    creds: PathBuf,
    plugin: PathBuf,
    selection: PathBuf,
}

impl Files {
    /// The credentials hold the passwords: never left behind.
    fn remove(&self, dir: Option<&PathBuf>) {
        for f in [&self.creds, &self.config, &self.selection] {
            let _ = fs::remove_file(f);
        }
        if let Some(d) = dir {
            let _ = fs::remove_dir_all(d);
        }
    }
}

fn argv(cmd: &Command) -> Vec<String> {
    std::iter::once(cmd.get_program()).chain(cmd.get_args()).map(|s| s.to_string_lossy().into_owned()).collect()
}

/// How it went: archinstall's exit, then what is in the new system (the
/// plugin marks setup finished in the user's home).
fn outcome(status: ExitStatus, user: &str) -> End {
    let mnt = Path::new("/mnt");
    crate::install_target::clear_grant(mnt); // archinstall has returned: no sudo rule may outlive it
    if !status.success() {
        return End::Failed(format!("archinstall stopped ({status}). Nothing after it ran."));
    }
    let Ok(passwd) = fs::read_to_string(mnt.join("etc/passwd")) else {
        // archinstall refuses some configs (a boot loader the layout can't
        // take) with a message and a clean exit.
        return End::Failed("archinstall ended without installing anything; its last lines say why.".into());
    };
    // Done, or set up with only session steps saved for first login.
    let done = crate::install_target::pick_user(&passwd, Some(user)).map(|a| {
        let state = mnt.join(a.home.trim_start_matches('/')).join(".local/state/bindle");
        state.join("first-login-done").exists() || state.join(crate::install_target::pending::SET_UP).exists()
    });
    let installed = mnt.join(firstlogin::BIN.trim_start_matches('/')).exists();
    match (installed, done) {
        (true, Some(true)) => End::Ready,
        (true, _) => End::Unfinished(
            "Arch is installed, but not every setup step finished. Bindle opens when you first log in and finishes the rest."
                .into(),
        ),
        (false, _) => End::Unfinished(format!(
            "Arch is installed, but Bindle wasn't set up in it. Before restarting: {} first-login install --root /mnt",
            firstlogin::BIN
        )),
    }
}

fn restart() -> Result<()> {
    let _ = Command::new("umount").args(["-R", "/mnt"]).status();
    Command::new("systemctl").arg("reboot").status().context("reboot")?;
    Ok(())
}

/// Without the progress screen: say how it went and offer the reboot.
fn finish_plain(end: End) -> Result<()> {
    match end {
        End::Failed(why) => bail!("{why} Log: /var/log/archinstall/install.log"),
        End::Ready => println!("\nDone. Your system is ready: remove the install media and restart."),
        End::Unfinished(why) => println!("\n{why}"),
    }
    print!("Restart now? [Y/n] ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    // No terminal any more (hung up): no answer, so no restart.
    let read = std::io::stdin().lock().read_line(&mut answer).unwrap_or(0);
    if read > 0 && matches!(answer.trim(), "" | "y" | "Y" | "yes") {
        restart()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_creds_hold_no_password() {
        let a = serde_json::json!({
            "users": [{ "username": "solon", "!password": "hunter22", "sudo": true }],
            "encryption_password": "disk-secret",
        });
        let text = redacted(a).to_string();
        assert!(!text.contains("hunter22") && !text.contains("disk-secret"), "{text}");
        assert!(text.contains("solon") && text.contains("left out"));
    }
}
