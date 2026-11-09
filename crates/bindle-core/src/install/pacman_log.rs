//! Rebuild the installer's choices from `pacman -r /mnt …` transactions in
//! /var/log/pacman.log. archinstall installs each feature in its own
//! transaction, so each one maps back to a menu choice.

use std::collections::BTreeSet;

use super::{GFX_DRIVERS, InstallRecord, Source};
use crate::desktops::{DESKTOP_BASE, Desktop, GREETERS};

const KERNELS: &[&str] = &["linux", "linux-lts", "linux-zen", "linux-hardened"];
const FONTS: &[&str] = &["noto-fonts", "noto-fonts-emoji", "noto-fonts-cjk", "ttf-liberation", "ttf-dejavu"];
/// pacman options whose value is the next argument (unless given as `--opt=value`).
const VALUE_OPTS: &[&str] = &[
    "-b",
    "-r",
    "--root",
    "--dbpath",
    "--config",
    "--cachedir",
    "--gpgdir",
    "--hookdir",
    "--logfile",
    "--arch",
    "--sysroot",
    "--ignore",
    "--ignoregroup",
    "--overwrite",
    "--assume-installed",
    "--assumeinstalled",
    "--print-format",
];

pub struct Transaction {
    pub timestamp: String,
    pub packages: BTreeSet<String>,
}

/// Installer-time transactions: pacman invoked with a non-`/` root.
pub fn transactions(log: &str) -> (Vec<Transaction>, bool) {
    let mut out = Vec::new();
    let mut archinstall_signature = false;
    for line in log.lines() {
        let line = line.trim_end();
        let Some(cmd) = line.split("Running '").nth(1).and_then(|c| c.strip_suffix('\'')) else { continue };
        let args: Vec<&str> = cmd.split_whitespace().collect();
        if args.first() != Some(&"pacman") {
            continue;
        }
        let root = args.iter().enumerate().find_map(|(i, a)| match *a {
            "-r" | "--root" => args.get(i + 1).copied(),
            a => a.strip_prefix("--root=").or_else(|| a.strip_prefix("-r").filter(|v| !v.is_empty())),
        });
        if root.is_none_or(|r| r.trim_end_matches('/').is_empty()) {
            continue;
        }
        // archinstall passes `-b <target>//var/lib/pacman/` and --disable-sandbox.
        archinstall_signature |= cmd.contains("//var/lib/pacman") || cmd.contains("--disable-sandbox");
        let mut packages = BTreeSet::new();
        let mut skip = false;
        for a in &args[1..] {
            if skip {
                skip = false;
            } else if VALUE_OPTS.contains(a) {
                skip = true;
            } else if !a.starts_with('-') {
                packages.insert(a.to_string());
            }
        }
        let timestamp = line.strip_prefix('[').and_then(|l| l.split(']').next()).unwrap_or_default().to_string();
        out.push(Transaction { timestamp, packages });
    }
    (out, archinstall_signature)
}

pub fn reconstruct(log: &str, desktops: &[Desktop]) -> Option<InstallRecord> {
    let (txs, archinstall) = transactions(log);
    let first = txs.first()?;
    let mut r = InstallRecord {
        source: Source::PacmanLog,
        installer: Some(if archinstall { "archinstall (likely)" } else { "pacstrap" }.into()),
        installed_at: Some(first.timestamp.clone()),
        ..Default::default()
    };
    for (i, tx) in txs.iter().enumerate() {
        let p = &tx.packages;
        let has = |n: &str| p.contains(n);
        let mut recognised = i == 0;
        if i == 0 {
            r.kernels = KERNELS.iter().filter(|k| has(k)).map(|k| k.to_string()).collect();
            if has("btrfs-progs") {
                r.root_fs = Some("btrfs".into());
            } else if has("xfsprogs") {
                r.root_fs = Some("xfs".into());
            }
        }
        let mut flag = |cond: bool, apply: &mut dyn FnMut()| {
            if cond {
                apply();
                recognised = true;
            }
        };
        flag(has("zram-generator"), &mut || r.zram = Some(true));
        flag(has("grub"), &mut || r.bootloader = Some("Grub".into()));
        flag(has("limine"), &mut || r.bootloader = Some("Limine".into()));
        flag(has("refind"), &mut || r.bootloader = Some("Refind".into()));
        flag(p.len() == 1 && has("efibootmgr"), &mut || {});
        flag(has("networkmanager") || has("iwd"), &mut || {
            r.network = Some(
                match (has("networkmanager"), has("iwd")) {
                    (true, true) => "nm_iwd",
                    (true, false) => "nm",
                    _ => "iwd",
                }
                .into(),
            );
        });
        flag(has("bluez"), &mut || r.bluetooth = Some(true));
        flag(has("pipewire"), &mut || r.audio = Some("pipewire".into()));
        flag(has("pulseaudio"), &mut || r.audio = Some("pulseaudio".into()));
        flag(has("power-profiles-daemon"), &mut || r.power = Some("power-profiles-daemon".into()));
        flag(has("tuned"), &mut || r.power = Some("tuned".into()));
        flag(has("ufw"), &mut || r.firewall = Some("ufw".into()));
        flag(has("firewalld"), &mut || r.firewall = Some("firewalld".into()));
        flag(has("cups"), &mut || r.printing = Some(true));
        flag(has("snapper"), &mut || r.snapshots = Some("Snapper".into()));
        flag(has("timeshift"), &mut || r.snapshots = Some("Timeshift".into()));
        flag(!p.is_empty() && p.iter().all(|x| FONTS.contains(&x.as_str())), &mut || {
            r.fonts = p.iter().cloned().collect();
        });
        flag(has("mesa") || has("nvidia-open-dkms"), &mut || r.gfx_driver = gfx_driver(p));
        flag(DESKTOP_BASE.iter().all(|b| has(b)), &mut || r.profile = Some("Desktop".into()));
        for d in desktops.iter().filter(|d| d.base.is_none()) {
            let flavor = ["plasma-meta", "plasma", "plasma-desktop"].into_iter().find(|f| has(f));
            let matched = if d.id == "plasma" { flavor.is_some() } else { d.archinstall.iter().all(|x| has(x)) };
            flag(matched, &mut || {
                if !r.desktops.contains(&d.id) {
                    r.desktops.push(d.id.clone());
                }
                if d.id == "plasma" {
                    r.plasma_flavor = flavor.map(String::from);
                }
            });
        }
        for (greeter, pkg) in GREETERS {
            flag(has(pkg), &mut || r.greeter = Some(greeter.to_string()));
        }
        if !recognised {
            r.extra_packages.extend(p.iter().cloned());
        }
    }
    if r.profile.is_none() && !r.desktops.is_empty() {
        r.profile = Some("Desktop".into());
    }
    Some(r)
}

fn gfx_driver(p: &BTreeSet<String>) -> Option<String> {
    // Vendor marker packages rather than exact sets: archinstall's package
    // lists change between releases (e.g. vpl-gpu-rt was added later).
    let has = |n: &str| p.contains(n);
    let (amd, intel, nouveau) = (has("vulkan-radeon"), has("vulkan-intel"), has("vulkan-nouveau"));
    let index = if has("nvidia-open-dkms") {
        3
    } else if amd && intel && nouveau {
        0
    } else if amd || has("xf86-video-amdgpu") {
        1
    } else if intel || has("intel-media-driver") {
        2
    } else if nouveau || has("xf86-video-nouveau") {
        4
    } else {
        5
    };
    Some(GFX_DRIVERS[index].0.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktops;

    fn line(ts: &str, pkgs: &str) -> String {
        format!(
            "[{ts}] [PACMAN] Running 'pacman -b /mnt//var/lib/pacman/ -r /mnt -Sy --noconfirm --needed --config=/tmp/pacman.conf.x --disable-sandbox --cachedir=/mnt//var/cache/pacman/pkg/ --noconfirm {pkgs}'\n[{ts}] [ALPM] installed foo (1-1)\n"
        )
    }

    fn gnome_install() -> String {
        [
            line("2026-09-10T11:26:50+0000", "base sudo linux-firmware mkinitcpio linux-zen btrfs-progs intel-ucode"),
            line("2026-09-10T11:27:42+0000", "zram-generator"),
            line("2026-09-10T11:27:46+0000", "efibootmgr"),
            line("2026-09-10T11:27:47+0000", "iwd"),
            line("2026-09-10T11:27:49+0000", "bluez bluez-utils"),
            line(
                "2026-09-10T11:27:50+0000",
                "pipewire pipewire-alsa pipewire-jack pipewire-pulse gst-plugin-pipewire libpulse wireplumber",
            ),
            line("2026-09-10T11:27:59+0000", "tuned tuned-ppd"),
            line("2026-09-10T11:28:04+0000", "ufw"),
            line("2026-09-10T11:28:05+0000", "noto-fonts noto-fonts-emoji noto-fonts-cjk ttf-liberation ttf-dejavu"),
            line("2026-09-10T11:28:12+0000", "mesa libva-intel-driver intel-media-driver vulkan-intel"),
            line("2026-09-10T11:28:14+0000", "nano vim openssh htop wget smartmontools xdg-utils"),
            line("2026-09-10T11:28:16+0000", "gnome gnome-tweaks"),
            line("2026-09-10T11:29:09+0000", "gdm"),
            line("2026-09-10T11:29:10+0000", "vivaldi vivaldi-ffmpeg-codecs"),
            "[2026-09-11T08:00:00+0000] [PACMAN] Running 'pacman -Syu'\n".into(),
        ]
        .concat()
    }

    #[test]
    fn rebuilds_a_gnome_archinstall() {
        let r = reconstruct(&gnome_install(), &desktops::all().unwrap()).unwrap();
        assert_eq!(r.installer.as_deref(), Some("archinstall (likely)"));
        assert_eq!(r.installed_at.as_deref(), Some("2026-09-10T11:26:50+0000"));
        assert_eq!(r.kernels, ["linux-zen"]);
        assert_eq!(r.root_fs.as_deref(), Some("btrfs"));
        assert_eq!(r.desktops, ["gnome"]);
        assert_eq!(r.greeter.as_deref(), Some("gdm"));
        assert_eq!(r.network.as_deref(), Some("iwd"));
        assert_eq!(r.power.as_deref(), Some("tuned"));
        assert_eq!(r.zram, Some(true));
        assert_eq!(r.fonts.len(), 5);
        assert_eq!(r.extra_packages, ["vivaldi", "vivaldi-ffmpeg-codecs"]);
        // Older archinstall: Intel set without vpl-gpu-rt/libvpl.
        assert_eq!(r.gfx_driver.as_deref(), Some("Intel (open-source)"));
    }

    #[test]
    fn ignores_regular_transactions() {
        assert!(reconstruct("[x] [PACMAN] Running 'pacman -Syu'\n", &desktops::all().unwrap()).is_none());
        let rooted = "[x] [PACMAN] Running 'pacman --root=/ -S vim'\n[y] [PACMAN] Running 'pacman -r / -S vim'\n";
        assert!(transactions(rooted).0.is_empty());
    }

    fn packages(log: &str) -> Vec<Vec<String>> {
        transactions(log).0.into_iter().map(|t| t.packages.into_iter().collect()).collect()
    }

    #[test]
    fn root_given_with_equals_or_attached() {
        let log = "[a] [PACMAN] Running 'pacman --root=/mnt -S grub'\n[b] [PACMAN] Running 'pacman -r/mnt -S bluez'\n";
        assert_eq!(packages(log), [["grub"], ["bluez"]]);
    }

    #[test]
    fn option_values_are_not_packages() {
        let log = "[a] [PACMAN] Running 'pacman -r /mnt -S --ignore linux --overwrite /usr/lib/* \
                   --assume-installed foo=1 --dbpath /mnt/var/lib/pacman --dbpath=/x --ignore=y \
                   --logfile /mnt/log --arch x86_64 --cachedir /c --gpgdir /g --hookdir /h ufw'\n";
        assert_eq!(packages(log), [["ufw"]]);
    }

    #[test]
    fn trailing_cr_and_whitespace() {
        let log = "[a] [PACMAN] Running 'pacman -r /mnt -S grub'\r\n[b] [PACMAN] Running 'pacman -r /mnt -S ufw'  \t\n\
                   [c] [PACMAN] Running 'pacman -r /mnt -S cups'\r";
        assert_eq!(packages(log), [["grub"], ["ufw"], ["cups"]]);
        let r = reconstruct(&gnome_install().replace('\n', "\r\n"), &desktops::all().unwrap()).unwrap();
        assert_eq!(r.desktops, ["gnome"]);
    }
}
