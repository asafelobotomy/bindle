//! A made-up system with something in every stage, for `--screenshots`
//! and the tests: the shots must not depend on the machine they run on.

use bindle_core::catalog::ReleaseAsset;
use bindle_core::whatsnew::Kind;
use bindle_upgrade::news::NewsItem;
use bindle_upgrade::parse::{Update, day};
use bindle_upgrade::probe::{After, Probe};
use bindle_upgrade::releases::{Newer, Source};
use bindle_upgrade::selfupdate::{Location, SelfState};
use bindle_upgrade::setup::Setup;
use bindle_upgrade::system::{Dkms, Kernel, Mount, Pin, Power};

fn up(name: &str, old: &str, new: &str) -> Update {
    Update { name: name.into(), old: old.into(), new: new.into() }
}

fn pinned(name: &str, tag: &str, asset: &str, installed: &str, source: Source) -> Newer {
    let asset = ReleaseAsset {
        repo: format!("asafelobotomy/{name}"),
        tag: tag.into(),
        asset: asset.into(),
        sha256: "0".repeat(64),
        name: name.into(),
        title: name.into(),
    };
    Newer { asset, installed: installed.into(), source }
}

/// The checks' progress lines, as the Checking page shows them.
pub const NOTES: [&str; 5] = [
    "Reading the Arch news",
    "Checking for package updates",
    "Checking the AUR",
    "Checking Flatpak",
    "Checking firmware (fwupd)",
];

pub fn probe() -> Probe {
    let today = day(2026, 11, 13);
    Probe {
        today,
        bindle: SelfState {
            running: "0.1.8".into(),
            latest: Some(Ok("v0.1.9".into())),
            location: Some(Location::Installed { gui: true }),
            just_updated: false,
        },
        mounts: vec![mount("/", "btrfs", 48_000), mount("/boot", "vfat", 640)],
        power: None,
        pin: None,
        pacman: Ok(vec![
            up("linux", "6.17.7.arch1-1", "6.17.8.arch1-1"),
            up("mesa", "1:25.2.6-1", "1:25.2.7-1"),
            up("firefox", "145.0-1", "145.0.1-1"),
            up("pipewire", "1:1.4.9-1", "1:1.4.10-1"),
            up("python", "3.13.9-1", "3.13.10-1"),
            up("kate", "25.08.3-1", "25.08.3-2"),
        ]),
        aur_helper: Some("paru".into()),
        aur: Ok(vec![up("visual-studio-code-bin", "1.106.0-1", "1.106.1-1")]),
        flatpak: Some(Ok(vec!["org.gimp.GIMP".into(), "org.freedesktop.Platform".into()])),
        firmware: Some(Ok(vec!["UEFI dbx → 371".into()])),
        firmware_age: Some(2),
        reflector: true,
        mirrorlist_age_days: Some(21),
        news: Ok(vec![
            NewsItem {
                title: "Valkey replaces Redis in [extra]".into(),
                link: "https://archlinux.org/news/valkey-replaces-redis/".into(),
                day: today - 30,
            },
            NewsItem {
                title: "zabbix 7.4 update requires manual intervention".into(),
                link: "https://archlinux.org/news/zabbix-74-update-requires-manual-intervention/".into(),
                day: today - 3,
            },
        ]),
        last_upgrade: Some(today - 12),
        releases: vec![
            pinned("xzram", "v0.4.4", "xzram-0.4.4-1-x86_64.pkg.tar.zst", "0.4.3-1", Source::Bindle),
            pinned("kyko", "v0.0.3", "kyko-0.0.3-1-x86_64.pkg.tar.zst", "0.0.2-1", Source::Other),
        ],
        after: After {
            pacfiles: vec!["/etc/pacman.conf.pacnew".into()],
            orphans: vec!["python-hatchling".into(), "python-installer".into()],
            running_kernel: "6.17.7-arch1-1".into(),
            kernel_modules_present: true,
            kernels: vec![kernel("6.17.7-arch1-1", "linux")],
            aur_helper: Some("paru".into()),
            setup: Setup {
                known: true,
                whats_new: vec![
                    (Kind::New, "Spell checking".into()),
                    (Kind::Updated, "Printing (CUPS)".into()),
                    (Kind::Optional, "Tab completion".into()),
                ],
                drift: vec![],
            },
            ..Default::default()
        },
        snapper_root: true,
        snap_pac: false,
    }
}

fn mount(target: &str, fstype: &str, avail_mb: u64) -> Mount {
    Mount { target: target.into(), fstype: fstype.into(), avail_mb }
}

fn kernel(version: &str, pkgbase: &str) -> Kernel {
    Kernel { version: version.into(), pkgbase: pkgbase.into() }
}

/// A laptop where the update must wait: /boot nearly full for a kernel
/// update, Hyprland past uQ's pin, on battery; and NVIDIA's DKMS module not
/// built for the kernel installed last time, so restarting isn't safe.
pub fn held() -> Probe {
    let mut p = probe();
    p.news = Ok(vec![]);
    p.pacman = Ok(vec![
        up("linux-zen", "7.3.4.zen2-1", "7.3.5.zen1-1"),
        up("hyprland", "0.57.1-1", "0.58.0-1"),
        up("firefox", "145.0-1", "145.0.1-1"),
    ]);
    p.bindle.latest = Some(Ok("v0.1.8".into()));
    p.mounts = vec![mount("/", "btrfs", 48_000), mount("/boot", "vfat", 84)];
    p.power = Some(Power { on_battery: true, capacity: Some(31) });
    p.pin = Some(Pin { at_least: Some("0.56.2".into()), below: Some("0.58.0".into()) });
    p.after.kernels = vec![kernel("7.3.4-zen2-1-zen", "linux-zen"), kernel("7.3.5-zen1-1-zen", "linux-zen")];
    p.after.dkms = vec![Dkms { module: "nvidia".into(), kernel: "7.3.4-zen2-1-zen".into(), state: "installed".into() }];
    p.after.running_kernel = "7.3.4-zen2-1-zen".into();
    p.after.kernel_modules_present = false;
    p.after.setup = Setup::default();
    p
}
