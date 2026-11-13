//! Look at the system without changing it: what can be updated, what the
//! news says, what is left over. Runs as the user; nothing here needs root.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::SystemTime;

use bindle_core::Catalog;
use bindle_core::catalog::ReleaseAsset;
use serde::Serialize;

use crate::news::{self, NewsItem};
use crate::parse::{self, Update};
use crate::prefs::Prefs;
use crate::releases::{self, Newer};
use crate::selfupdate::SelfState;
use crate::setup::Setup;
use crate::stage::Stage;
use crate::system::{Dkms, Kernel, Mount, Pin, Power, Rebuild};

#[path = "probe_system.rs"]
mod look;

/// A list, or why it couldn't be made.
pub type Listed<T> = Result<Vec<T>, String>;

#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    /// Days since 1970-01-01.
    pub today: i64,
    /// This Bindle and its latest release.
    pub bindle: SelfState,
    /// Free space on `/`, `/boot` and the EFI partition.
    pub mounts: Vec<Mount>,
    /// None: no battery (a desktop).
    pub power: Option<Power>,
    /// uq-hyprland-pin's range, when it is installed.
    pub pin: Option<Pin>,
    pub pacman: Listed<Update>,
    /// `paru` or `yay`, when installed.
    pub aur_helper: Option<String>,
    pub aur: Listed<Update>,
    /// None: Flatpak isn't installed.
    pub flatpak: Option<Listed<String>>,
    /// None: fwupd isn't installed.
    pub firmware: Option<Listed<String>>,
    /// Age in days of fwupd's firmware list (after any refresh).
    pub firmware_age: Option<i64>,
    pub reflector: bool,
    pub mirrorlist_age_days: Option<i64>,
    /// The whole feed; the plan picks what is new.
    pub news: Listed<NewsItem>,
    /// Day of the last `pacman -Syu`, from pacman.log.
    pub last_upgrade: Option<i64>,
    /// Installed apps with a newer pin, and where each came from.
    pub releases: Vec<Newer>,
    pub after: After,
    /// snapper has a config for `/`.
    pub snapper_root: bool,
    /// snap-pac already snapshots around every pacman run.
    pub snap_pac: bool,
}

/// What is looked at again once the upgrades ran.
#[derive(Debug, Clone, Default, Serialize)]
pub struct After {
    pub pacfiles: Vec<String>,
    pub orphans: Vec<String>,
    pub running_kernel: String,
    /// `/usr/lib/modules/<running kernel>` still exists (a kernel upgrade
    /// removes it, which is how a pending reboot shows).
    pub kernel_modules_present: bool,
    /// Installed kernels (/usr/lib/modules with a `pkgbase`).
    pub kernels: Vec<Kernel>,
    /// `dkms status`; empty without DKMS.
    pub dkms: Vec<Dkms>,
    /// The files in /boot; None when /boot can't be read (often root-only).
    pub boot: Option<Vec<String>>,
    /// Failed system and user units.
    pub failed: Vec<String>,
    /// `checkrebuild`; None when rebuild-detector isn't installed.
    pub rebuild: Option<Vec<Rebuild>>,
    pub aur_helper: Option<String>,
    pub uq: bool,
    /// greetd (uQ's login screen) is the display manager.
    pub greeter: bool,
    pub setup: Setup,
}

impl Default for Probe {
    /// Everything up to date and nothing installed: a base for tests.
    fn default() -> Self {
        Probe {
            today: 0,
            bindle: SelfState::default(),
            mounts: vec![],
            power: None,
            pin: None,
            pacman: Ok(vec![]),
            aur_helper: None,
            aur: Ok(vec![]),
            flatpak: None,
            firmware: None,
            firmware_age: None,
            reflector: false,
            mirrorlist_age_days: None,
            news: Ok(vec![]),
            last_upgrade: None,
            releases: vec![],
            after: After { kernel_modules_present: true, ..Default::default() },
            snapper_root: false,
            snap_pac: false,
        }
    }
}

/// Exit code, stdout, stderr; an error when it can't start.
fn capture(argv: &[&str]) -> Result<(i32, String, String), String> {
    let (prog, args) = argv.split_first().ok_or("empty command")?;
    let out =
        Command::new(prog).args(args).stdin(Stdio::null()).output().map_err(|e| format!("can't run {prog}: {e}"))?;
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    Ok((out.status.code().unwrap_or(-1), text(&out.stdout), text(&out.stderr)))
}

pub fn on_path(prog: &str) -> bool {
    env::var_os("PATH").is_some_and(|p| env::split_paths(&p).any(|d| d.join(prog).is_file()))
}

fn first_line(s: &str) -> String {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("no output").trim().to_string()
}

pub fn today() -> i64 {
    let secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    (secs / 86_400) as i64
}

/// Look at everything the enabled stages need; `note` hears what is being
/// checked (some checks take a few seconds).
/// `bindle`: the `bindle` program this run belongs to (whether it can update
/// itself depends on where it is). `refresh`: may refresh fwupd's firmware
/// list when it is old (a download into fwupd's cache); a `--check` doesn't.
pub fn gather(
    catalog: &Catalog,
    prefs: &Prefs,
    bindle: &Path,
    refresh: bool,
    enabled: &dyn Fn(Stage) -> bool,
    note: &mut dyn FnMut(&str),
) -> Probe {
    let mut p = Probe { today: today(), ..Default::default() };
    let off = || Err("not checked".to_string());
    if enabled(Stage::Bindle) {
        note("Checking for a new Bindle");
        p.bindle = look::bindle(bindle);
    }
    if enabled(Stage::News) {
        note("Reading the Arch news");
        p.news = arch_news();
        p.last_upgrade = fs::read_to_string("/var/log/pacman.log").ok().and_then(|l| parse::last_upgrade(&l));
    }
    if enabled(Stage::Pacman) {
        note("Checking for package updates");
        p.pacman = pacman_updates();
    } else {
        p.pacman = off();
    }
    p.aur_helper = ["paru", "yay"].into_iter().find(|h| on_path(h)).map(String::from);
    p.aur = match (&p.aur_helper, enabled(Stage::Aur)) {
        (Some(h), true) => {
            note("Checking the AUR");
            aur_updates(h)
        }
        _ => off(),
    };
    if enabled(Stage::Flatpak) && on_path("flatpak") {
        note("Checking Flatpak");
        p.flatpak = Some(flatpak_updates());
    }
    if enabled(Stage::Firmware) && on_path("fwupdmgr") {
        note("Checking firmware (fwupd)");
        p.firmware_age = look::firmware(prefs.firmware_refresh_days, refresh);
        p.firmware = Some(firmware_updates());
    }
    p.mounts = look::mounts();
    p.power = look::power();
    p.reflector = on_path("reflector");
    p.mirrorlist_age_days = age_days(Path::new("/etc/pacman.d/mirrorlist"));
    let installed = installed_versions();
    p.snapper_root = Path::new("/etc/snapper/configs/root").is_file();
    p.snap_pac = installed.contains_key("snap-pac");
    if installed.contains_key("uq-hyprland-pin") {
        p.pin = Some(look::pin());
    }
    if enabled(Stage::Releases) {
        p.releases = release_updates(catalog, &installed);
    }
    note("Looking at kernels, services and Bindle's setup");
    p.after = after();
    p
}

/// Leftovers and the kernel, looked at before and again after upgrading.
pub fn after() -> After {
    let running = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim().to_string();
    After {
        pacfiles: pacfiles(),
        orphans: capture(&["pacman", "-Qdtq"])
            .map(|(_, out, _)| out.lines().map(String::from).collect())
            .unwrap_or_default(),
        kernel_modules_present: running.is_empty() || Path::new("/usr/lib/modules").join(&running).is_dir(),
        running_kernel: running,
        kernels: look::kernels(),
        dkms: look::dkms(),
        boot: look::boot(),
        failed: look::failed(),
        rebuild: look::rebuild(),
        aur_helper: ["paru", "yay"].into_iter().find(|h| on_path(h)).map(String::from),
        uq: on_path("uqctl"),
        greeter: look::greeter(),
        setup: crate::setup::look(),
    }
}

fn arch_news() -> Listed<NewsItem> {
    let argv = ["curl", "-fsSL", "--max-time", "20", "--proto", "=https", "--tlsv1.2", news::FEED];
    match capture(&argv)? {
        (0, xml, _) => Ok(news::parse(&xml)),
        (_, _, err) => Err(first_line(&err)),
    }
}

/// `checkupdates` (pacman-contrib) syncs a private copy of the databases,
/// so it needs no root and leaves pacman's own alone. Exit 2: none.
fn pacman_updates() -> Listed<Update> {
    if !on_path("checkupdates") {
        return Err("pacman-contrib isn't installed (it has checkupdates)".into());
    }
    match capture(&["checkupdates"])? {
        (0, out, _) => Ok(parse::updates(&out)),
        (2, _, _) => Ok(vec![]),
        (_, _, err) => Err(first_line(&err)),
    }
}

fn aur_updates(helper: &str) -> Listed<Update> {
    match capture(&[helper, "-Qua"])? {
        (0, out, _) => Ok(parse::updates(&out)),
        // Both exit 1 when there is nothing to update.
        (1, out, err) if out.trim().is_empty() && !err.to_lowercase().contains("error") => Ok(vec![]),
        (_, _, err) => Err(first_line(&err)),
    }
}

fn flatpak_updates() -> Listed<String> {
    match capture(&["flatpak", "remote-ls", "--updates", "--columns=application"])? {
        (0, out, _) => Ok(parse::flatpak_apps(&out)),
        (_, _, err) => Err(first_line(&err)),
    }
}

fn firmware_updates() -> Listed<String> {
    match capture(&["fwupdmgr", "get-updates", "--json"])? {
        (0, out, _) => parse::firmware(&out),
        // 2: nothing to do.
        (2, _, _) => Ok(vec![]),
        (_, out, err) => Err(first_line(if err.trim().is_empty() { &out } else { &err })),
    }
}

fn age_days(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(SystemTime::now().duration_since(modified).map_or(0, |d| d.as_secs() / 86_400) as i64)
}

fn installed_versions() -> BTreeMap<String, String> {
    capture(&["pacman", "-Q"])
        .map(|(_, out, _)| {
            out.lines().filter_map(|l| l.split_once(' ')).map(|(n, v)| (n.to_string(), v.to_string())).collect()
        })
        .unwrap_or_default()
}

fn release_updates(catalog: &Catalog, installed: &BTreeMap<String, String>) -> Vec<Newer> {
    let home = env::var_os("HOME").map(std::path::PathBuf::from);
    let at_least = |a: &str, b: &str| {
        capture(&["vercmp", a, b]).is_ok_and(|(_, out, _)| out.trim().parse::<i32>().is_ok_and(|c| c >= 0))
    };
    let entry = |a: &ReleaseAsset| {
        let home = home.as_ref()?;
        fs::read_to_string(home.join(a.desktop_entry().trim_start_matches("~/"))).ok()
    };
    let foreign: BTreeSet<String> =
        capture(&["pacman", "-Qmq"]).map(|(_, out, _)| out.lines().map(String::from).collect()).unwrap_or_default();
    let recorded: BTreeSet<String> = bindle_run::snapshot::list(Path::new("/"))
        .into_iter()
        .flat_map(|m| m.releases.into_iter().chain(m.packages))
        .collect();
    let source = |n: &str| releases::classify(n, &foreign, &recorded);
    releases::newer(&releases::pinned(catalog), &|n| installed.get(n).cloned(), &at_least, &source, &entry)
}

/// `.pacnew` and `.pacsave` files: pacdiff knows where to look (from
/// pacman's database); without it, a search of /etc.
fn pacfiles() -> Vec<String> {
    let found = if on_path("pacdiff") {
        capture(&["pacdiff", "--output"])
    } else {
        capture(&["find", "/etc", "(", "-name", "*.pacnew", "-o", "-name", "*.pacsave", ")", "-print"])
    };
    let mut files: Vec<String> = found
        .map(|(_, out, _)| out.lines().map(str::trim).filter(|l| l.starts_with('/')).map(String::from).collect())
        .unwrap_or_default();
    files.sort();
    files.dedup();
    files
}
