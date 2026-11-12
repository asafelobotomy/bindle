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
use std::os::unix::fs::{DirBuilderExt, FileTypeExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use bindle_core::install::preset::{self, IsoChoice, TESTED_ARCHINSTALL};
use bindle_core::{Catalog, Facts, desktops};
use serde_json::Value;

use crate::apply::sys::{is_root, write_atomic};
use crate::archinstall_version::{archinstall_version, version_note};
use crate::firstlogin;
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
    /// Leave the profile for the first-login wizard instead of applying it
    /// in the new system before reboot.
    pub setup_at_first_login: bool,
    pub workdir: Option<PathBuf>,
}

fn online() -> bool {
    ("archlinux.org", 443)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .is_some_and(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(5)).is_ok())
}

/// Why this isn't the Arch live ISO, or None if it is: `/run/archiso`
/// exists, the kernel was booted with `archisobasedir=`, and `/` is the
/// archiso overlay (`airootfs`). Pure, so it can be tested with fixtures.
fn not_live_iso(run_archiso: bool, cmdline: &str, mounts: &str) -> Option<&'static str> {
    if !run_archiso {
        return Some("/run/archiso is missing");
    }
    if !cmdline.split_whitespace().any(|w| w.starts_with("archisobasedir=")) {
        return Some("the kernel command line has no archisobasedir=");
    }
    // The last mount on / is the visible one.
    let root = mounts.lines().rev().find_map(|l| {
        let mut f = l.split_whitespace();
        let (dev, at, fstype) = (f.next()?, f.next()?, f.next()?);
        (at == "/").then_some((dev, fstype))
    });
    match root {
        Some((dev, fstype)) if fstype == "overlay" || dev == "airootfs" => None,
        _ => Some("the root filesystem isn't the archiso overlay (airootfs)"),
    }
}

fn is_live_iso() -> Result<()> {
    let read = |p: &str| fs::read_to_string(p).unwrap_or_default();
    match not_live_iso(Path::new("/run/archiso").is_dir(), &read("/proc/cmdline"), &read("/proc/self/mounts")) {
        None => Ok(()),
        Some(why) => {
            bail!("`bindle iso` installs Arch and wipes disks; it only runs from the Arch Linux live ISO ({why})")
        }
    }
}

/// The devices an unattended install will modify, from the merged config's
/// `disk_config.device_modifications`. Silent mode never starts without them.
fn silent_targets(cfg: &Value) -> Result<Vec<String>> {
    let mods = cfg.pointer("/disk_config/device_modifications").and_then(Value::as_array);
    let Some(mods) = mods.filter(|m| !m.is_empty()) else {
        bail!("--silent needs a --config with a non-empty disk_config.device_modifications (the disks to install to)");
    };
    mods.iter()
        .map(|m| match m.get("device").and_then(Value::as_str) {
            Some(d) if !d.is_empty() => Ok(d.to_string()),
            _ => bail!("disk_config.device_modifications has an entry without a \"device\""),
        })
        .collect()
}

fn check_block_devices(devices: &[String], is_block: impl Fn(&Path) -> bool) -> Result<()> {
    for d in devices {
        if !is_block(Path::new(d)) {
            bail!("{d} (from disk_config.device_modifications) is not a block device on this machine");
        }
    }
    Ok(())
}

fn is_block_device(p: &Path) -> bool {
    fs::metadata(p).is_ok_and(|m| m.file_type().is_block_device())
}

fn preflight(dry_run: bool) -> Result<Option<String>> {
    if dry_run {
        return Ok(archinstall_version());
    }
    is_live_iso()?;
    if !is_root() {
        bail!("run as root (the live ISO shell already is)");
    }
    if !online() {
        bail!("no internet connection. Wi-Fi: `iwctl station <device> connect <SSID>`; then run Bindle again");
    }
    let version = archinstall_version().context("archinstall not found on this ISO")?;
    Ok(Some(version))
}

/// Bail on ids that don't exist, before anything is written or started.
fn validate(catalog: &Catalog, data: &[desktops::Desktop], opts: &IsoOpts) -> Result<()> {
    if let Some(id) = opts.desktop.as_deref() {
        match desktops::by_id(data, id) {
            None => {
                let ids: Vec<&str> = data.iter().map(|d| d.id.as_str()).collect();
                bail!("unknown desktop `{id}`; choose from: {}", ids.join(", "));
            }
            Some(d) => {
                if let Some(why) = &d.broken {
                    bail!("{} can't be installed right now: {why}", d.name);
                }
            }
        }
    }
    if let Some(p) = opts.profile.as_deref()
        && !catalog.profiles.iter().any(|x| x.id == p)
    {
        let ids: Vec<&str> = catalog.profiles.iter().map(|x| x.id.as_str()).collect();
        bail!("unknown profile `{p}`; choose from: {}", ids.join(", "));
    }
    Ok(())
}

/// A fresh private directory for the preset (it can hold a merged config with
/// disk layout and more): never a predictable, pre-creatable /tmp path.
fn private_workdir() -> Result<PathBuf> {
    let base = if is_root() && Path::new("/run").is_dir() { PathBuf::from("/run") } else { std::env::temp_dir() };
    for attempt in 0..16u32 {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
        let dir = base.join(format!("bindle-iso-{}-{nonce:08x}{attempt}", std::process::id()));
        match fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("create {}", dir.display())),
        }
    }
    bail!("could not create a private directory under {}", base.display())
}

pub fn run(catalog: &Catalog, facts: &Facts, opts: IsoOpts) -> Result<()> {
    if opts.silent && (opts.config.is_none() || opts.creds.is_none() || !opts.yes || opts.profile.is_none()) {
        bail!("--silent needs --config (with disk layout), --creds, --profile and --yes");
    }
    let version = preflight(opts.dry_run)?;
    if let Some(note) = version.as_deref().and_then(|v| version_note(v, TESTED_ARCHINSTALL)) {
        println!("{note}");
    }

    let data = desktops::all()?;
    validate(catalog, &data, &opts)?;
    let overlay: Option<Value> = match &opts.config {
        Some(path) => {
            let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
            Some(serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?)
        }
        None => None,
    };
    let choice = match (&opts.profile, opts.yes) {
        (Some(profile), true) => IsoChoice { desktop: opts.desktop.clone(), profile: profile.clone() },
        _ => {
            if !std::io::stdout().is_terminal() {
                bail!("needs a terminal, or pass --profile … --yes");
            }
            match tui::iso::run(
                facts,
                &data,
                &catalog.profiles,
                opts.desktop.clone(),
                opts.profile.clone(),
                overlay.as_ref(),
            )? {
                Some(c) => c,
                None => {
                    println!("Cancelled. Nothing was changed.");
                    return Ok(());
                }
            }
        }
    };

    let mut preset = preset::build(facts, &data, &choice, overlay.as_ref());
    // The desktop window at first login needs Qt Quick and Kirigami.
    let window = std::env::current_exe().map(|e| e.with_file_name("bindle-gui").is_file()).unwrap_or(false);
    if window && choice.profile != "minimal" {
        add_packages(&mut preset, WINDOW_RUNTIME);
    }
    let targets = if opts.silent { silent_targets(&preset)? } else { Vec::new() };
    if !opts.dry_run {
        check_block_devices(&targets, is_block_device)?;
    }

    let work = match &opts.workdir {
        Some(w) => {
            crate::apply::sys::create_dirs(w)?;
            w.clone()
        }
        None => private_workdir()?,
    };
    let config = work.join("bindle-archinstall.json");
    let plugin = work.join("bindle_plugin.py");
    let sel = work.join("selection.toml");
    write_atomic(&config, serde_json::to_string_pretty(&preset)?.as_bytes(), 0o600)?;
    write_atomic(&plugin, PLUGIN.as_bytes(), 0o600)?;
    let picked = preset::first_login_selection(&data, &choice);
    write_atomic(&sel, selection::render(&picked)?.as_bytes(), 0o600)?;

    let exe = std::env::current_exe()?;
    let mut cmd = Command::new("archinstall");
    cmd.arg("--config").arg(&config).arg("--plugin").arg(&plugin).env("BINDLE_BIN", &exe).env("BINDLE_SELECTION", &sel);
    if opts.keep_welcome {
        cmd.env("BINDLE_KEEP_WELCOME", "1");
    }
    if !opts.setup_at_first_login {
        cmd.env("BINDLE_SETUP_TARGET", "1");
    }
    if let Some(creds) = &opts.creds {
        cmd.arg("--creds").arg(creds);
    }
    if opts.silent {
        cmd.arg("--silent");
    }
    if !targets.is_empty() {
        println!("Unattended install to: {} (everything on these disks may be erased)", targets.join(", "));
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
    let status = cmd.status();
    // The merged config can hold the disk layout and more; the plugin has
    // read the selection by now. Remove both whatever archinstall did.
    for f in [&config, &sel] {
        let _ = fs::remove_file(f);
    }
    if opts.workdir.is_none() {
        let _ = fs::remove_dir_all(&work);
    }
    let status = status.context("start archinstall")?;
    if !status.success() {
        bail!("archinstall exited with {status}. Log: /var/log/archinstall/install.log");
    }
    let installed = |p: &str| crate::apply::sys::host(Path::new("/mnt"), p).exists();
    if installed(firstlogin::BIN) && installed(firstlogin::AUTOSTART) {
        if opts.setup_at_first_login {
            println!("\nDone. Reboot and remove the install media; Bindle opens when you first log in");
            println!("with the `{}` profile ready to review.", choice.profile);
        } else {
            println!("\nDone. Reboot and remove the install media. The `{}` profile is installed;", choice.profile);
            println!(
                "if a step didn't finish (see the [bindle] lines above), Bindle opens at first login to finish it."
            );
        }
    } else {
        println!("\narchinstall finished, but Bindle wasn't set up in the new system (see the [bindle] lines above).");
        println!("If it's still mounted at /mnt: {} first-login install --root /mnt", exe.display());
    }
    Ok(())
}

/// What `bindle-gui` needs on the installed system (breeze-icons: its icons).
const WINDOW_RUNTIME: &[&str] = &["qt6-declarative", "kirigami", "breeze-icons"];

/// Append to archinstall's extra `packages`, keeping any from `--config`.
fn add_packages(preset: &mut Value, pkgs: &[&str]) {
    let list = preset["packages"].as_array().cloned().unwrap_or_default();
    let mut out: Vec<Value> = list;
    for p in pkgs {
        if !out.iter().any(|v| v == p) {
            out.push(Value::from(*p));
        }
    }
    preset["packages"] = Value::Array(out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_runtime_joins_the_users_packages() {
        let mut p = json!({ "packages": ["vim", "kirigami"] });
        add_packages(&mut p, WINDOW_RUNTIME);
        assert_eq!(p["packages"], json!(["vim", "kirigami", "qt6-declarative", "breeze-icons"]));
        let mut p = json!({});
        add_packages(&mut p, WINDOW_RUNTIME);
        assert_eq!(p["packages"], json!(["qt6-declarative", "kirigami", "breeze-icons"]));
    }
    use serde_json::json;

    const ISO_CMDLINE: &str =
        "BOOT_IMAGE=/arch/boot/x86_64/vmlinuz-linux archisobasedir=arch archisosearchuuid=2026-11-01 cow_spacesize=4G";
    const ISO_MOUNTS: &str = "proc /proc proc rw 0 0\nairootfs / overlay rw,relatime,lowerdir=/run/archiso/sfs/airootfs,upperdir=/run/archiso/cowspace/persistent_/x86_64/upperdir 0 0\n/dev/sr0 /run/archiso/bootmnt iso9660 ro 0 0\n";
    const HOST_MOUNTS: &str = "/dev/nvme0n1p2 / btrfs rw,subvol=/@ 0 0\n/dev/nvme0n1p1 /boot vfat rw 0 0\n";

    #[test]
    fn live_iso_needs_all_three_signals() {
        assert_eq!(not_live_iso(true, ISO_CMDLINE, ISO_MOUNTS), None);
        assert!(not_live_iso(false, ISO_CMDLINE, ISO_MOUNTS).unwrap().contains("/run/archiso"));
        let host_cmdline = "BOOT_IMAGE=/vmlinuz-linux root=UUID=abc rw quiet";
        assert!(not_live_iso(true, host_cmdline, ISO_MOUNTS).unwrap().contains("archisobasedir"));
        assert!(not_live_iso(true, ISO_CMDLINE, HOST_MOUNTS).unwrap().contains("root filesystem"));
        assert!(not_live_iso(true, ISO_CMDLINE, "").is_some());
        // A prefix isn't enough; the parameter must be its own word.
        assert!(not_live_iso(true, "foo.archisobasedir=arch", ISO_MOUNTS).is_some());
    }

    #[test]
    fn last_root_mount_wins() {
        let stacked = format!("{ISO_MOUNTS}/dev/sda2 / ext4 rw 0 0\n");
        assert!(not_live_iso(true, ISO_CMDLINE, &stacked).is_some());
        assert_eq!(not_live_iso(true, ISO_CMDLINE, "airootfs / tmpfs rw 0 0\n"), None);
    }

    #[test]
    fn silent_needs_target_devices() {
        assert!(silent_targets(&json!({})).is_err());
        assert!(silent_targets(&json!({"disk_config": {"config_type": "default_layout"}})).is_err());
        assert!(silent_targets(&json!({"disk_config": {"device_modifications": []}})).is_err());
        assert!(silent_targets(&json!({"disk_config": {"device_modifications": [{"wipe": true}]}})).is_err());
        let cfg = json!({"disk_config": {"device_modifications": [
            {"device": "/dev/vda", "wipe": true}, {"device": "/dev/vdb", "wipe": false}
        ]}});
        assert_eq!(silent_targets(&cfg).unwrap(), ["/dev/vda", "/dev/vdb"]);
    }

    #[test]
    fn target_devices_must_be_block_devices() {
        let devs = vec!["/dev/vda".to_string(), "/dev/vdb".to_string()];
        assert!(check_block_devices(&devs, |_| true).is_ok());
        let err = check_block_devices(&devs, |p| p == Path::new("/dev/vda")).unwrap_err();
        assert!(err.to_string().contains("/dev/vdb"));
        assert!(!is_block_device(Path::new("/proc/cmdline")));
    }
}
