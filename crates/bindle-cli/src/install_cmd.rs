//! `bindle install` (also `bindleinstall`): the whole install from the live
//! ISO in Bindle's own wizard. archinstall still does the disks, bootloader
//! and base system, unattended (`--silent`) from the answers given here; then
//! Bindle's plugin carries the Wi-Fi over and applies the chosen profile in
//! the new system, so the first boot lands in a finished desktop.

use std::fs;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::ExitStatus;

use anyhow::{Context, Result, bail};
use bindle_core::install::target;
use bindle_core::{Catalog, Facts, desktops};

use crate::apply::sys::{is_root, write_atomic};
use crate::archinstall_version::{archinstall_version, version_note};
use crate::iso::{PLUGIN, WINDOW_RUNTIME, add_packages, is_live_iso, private_workdir};
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
    let Some(out) = tui::install::run(facts, catalog, &data)? else {
        println!("Cancelled. Nothing was changed.");
        return Ok(());
    };

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
    write_atomic(&files.config, serde_json::to_string_pretty(&config)?.as_bytes(), 0o600)?;
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
            "Wrote {}, {}, {} and {}.",
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
            let status = cmd.status();
            files.remove(opts.workdir.is_none().then_some(&work));
            return finish_plain(outcome(status.context("start archinstall")?, &user));
        }
        Err(e) => {
            files.remove(opts.workdir.is_none().then_some(&work));
            return Err(e);
        }
    };
    files.remove(opts.workdir.is_none().then_some(&work));
    keep_log();
    match done.end {
        End::Failed(why) => {
            done.tail.iter().for_each(|l| println!("{l}"));
            bail!("{why}\nEverything it printed: {LOG}");
        }
        End::Ready | End::Unfinished(_) if done.restart => restart()?,
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
    if !status.success() {
        return End::Failed(format!("archinstall stopped ({status}). Nothing after it ran."));
    }
    let Ok(passwd) = fs::read_to_string(mnt.join("etc/passwd")) else {
        // archinstall refuses some configs (a boot loader the layout can't
        // take) with a message and a clean exit.
        return End::Failed("archinstall ended without installing anything; its last lines say why.".into());
    };
    let done = crate::install_target::pick_user(&passwd, Some(user))
        .map(|a| mnt.join(a.home.trim_start_matches('/')).join(".local/state/bindle/first-login-done").exists());
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
    let _ = std::io::stdin().lock().read_line(&mut answer);
    if matches!(answer.trim(), "" | "y" | "Y" | "yes") {
        restart()?;
    }
    Ok(())
}
