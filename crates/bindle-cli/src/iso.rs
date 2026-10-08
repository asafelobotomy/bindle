//! `bindle iso`: run from the Arch live ISO instead of archinstall.
//!
//! 1. Preflight (live ISO, root, network, archinstall present).
//! 2. Pre-install screen: desktop + Bindle profile.
//! 3. archinstall with a hardware-aware preset (`--config`) and the Bindle
//!    plugin (`--plugin`), which installs Bindle into the new system so it
//!    opens on first login with the chosen profile.

use std::fs;
use std::io::IsTerminal;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use bindle_core::install::preset::{self, IsoChoice, TESTED_ARCHINSTALL};
use bindle_core::{Facts, Selection, desktops};

use crate::apply::sys::is_root;
use crate::{selection, tui};

const PLUGIN: &str = include_str!("../../../packaging/archinstall/bindle_plugin.py");

pub struct IsoOpts {
    pub desktop: Option<String>,
    pub profile: Option<String>,
    pub yes: bool,
    pub dry_run: bool,
    pub config: Option<PathBuf>,
    pub creds: Option<PathBuf>,
    pub silent: bool,
    pub keep_welcome: bool,
    pub workdir: Option<PathBuf>,
}

fn online() -> bool {
    ("archlinux.org", 443)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .is_some_and(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(5)).is_ok())
}

fn archinstall_version() -> Option<String> {
    let out = Command::new("archinstall").arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit())).map(String::from)
}

fn major_minor(v: &str) -> (u32, u32) {
    let mut it = v.split('.').map(|p| p.parse().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

fn preflight(dry_run: bool) -> Result<Option<String>> {
    if dry_run {
        return Ok(archinstall_version());
    }
    if !Path::new("/run/archiso").exists() {
        bail!("`bindle iso` installs Arch and wipes disks; it only runs from the Arch Linux live ISO");
    }
    if !is_root() {
        bail!("run as root (the live ISO shell already is)");
    }
    if !online() {
        bail!("no internet connection. Wi-Fi: `iwctl station <device> connect <SSID>`; then run Bindle again");
    }
    let version = archinstall_version().context("archinstall not found on this ISO")?;
    Ok(Some(version))
}

pub fn run(facts: &Facts, opts: IsoOpts) -> Result<()> {
    if opts.silent && (opts.config.is_none() || opts.creds.is_none() || !opts.yes || opts.profile.is_none()) {
        bail!("--silent needs --config (with disk layout), --creds, --profile and --yes");
    }
    let version = preflight(opts.dry_run)?;
    if let Some(v) = &version
        && major_minor(v) > major_minor(TESTED_ARCHINSTALL)
    {
        println!(
            "Note: archinstall {v} is newer than {TESTED_ARCHINSTALL}, which Bindle was tested with. \
             archinstall may log that the Bindle plugin \"does not support\" it; the plugin only uses \
             the long-stable on_install hook."
        );
    }

    let data = desktops::all()?;
    if let Some(d) = opts.desktop.as_deref().and_then(|id| desktops::by_id(&data, id))
        && let Some(why) = &d.broken
    {
        bail!("{} can't be installed right now: {why}", d.name);
    }
    let choice = match (&opts.profile, opts.yes) {
        (Some(profile), true) => IsoChoice { desktop: opts.desktop.clone(), profile: profile.clone() },
        _ => {
            if !std::io::stdout().is_terminal() {
                bail!("needs a terminal, or pass --profile … --yes");
            }
            match tui::iso::run(facts, &data, opts.desktop.clone(), opts.profile.clone())? {
                Some(c) => c,
                None => {
                    println!("Cancelled. Nothing was changed.");
                    return Ok(());
                }
            }
        }
    };

    let work = match &opts.workdir {
        Some(w) => w.clone(),
        None => std::env::temp_dir().join("bindle-iso"),
    };
    fs::create_dir_all(&work)?;
    let mut preset = preset::archinstall_preset(facts, &data, &choice);
    if let Some(path) = &opts.config {
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let overlay = serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        preset::merge(&mut preset, overlay);
    }
    let config = work.join("bindle-archinstall.json");
    let plugin = work.join("bindle_plugin.py");
    let sel = work.join("selection.toml");
    fs::write(&config, serde_json::to_string_pretty(&preset)?)?;
    fs::write(&plugin, PLUGIN)?;
    selection::save(&Selection { profile: Some(choice.profile.clone()), ..Default::default() }, &sel)?;

    let exe = std::env::current_exe()?;
    let mut cmd = Command::new("archinstall");
    cmd.arg("--config").arg(&config).arg("--plugin").arg(&plugin).env("BINDLE_BIN", &exe).env("BINDLE_SELECTION", &sel);
    if opts.keep_welcome {
        cmd.env("BINDLE_KEEP_WELCOME", "1");
    }
    if let Some(creds) = &opts.creds {
        cmd.arg("--creds").arg(creds);
    }
    if opts.silent {
        cmd.arg("--silent");
    }
    if opts.dry_run {
        println!("Wrote {}, {} and {}.", config.display(), plugin.display(), sel.display());
        println!(
            "Would run: BINDLE_BIN={} BINDLE_SELECTION={} archinstall --config {} --plugin {}",
            exe.display(),
            sel.display(),
            config.display(),
            plugin.display()
        );
        return Ok(());
    }

    if opts.silent {
        println!("\nStarting unattended archinstall with Bindle's presets…\n");
    } else {
        println!("\nStarting archinstall with Bindle's presets. Set disks, users, hostname and locale there;");
        println!("everything else is pre-filled and can still be changed.\n");
    }
    let status = cmd.status().context("start archinstall")?;
    if !status.success() {
        bail!("archinstall exited with {status}. Log: /var/log/archinstall/install.log");
    }
    if Path::new("/mnt/usr/local/bin/bindle").exists() {
        println!("\nDone. Reboot and remove the install media; Bindle opens when you first log in");
        println!("with the `{}` profile ready to review.", choice.profile);
    } else {
        println!("\narchinstall finished, but Bindle wasn't installed into the new system.");
        println!("If it's still mounted at /mnt: {} first-login install --root /mnt", exe.display());
    }
    Ok(())
}
