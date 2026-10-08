//! archinstall logs its full chosen configuration (credentials stripped) as a
//! DEBUG entry in install.log, and copies that log into the installed system.

use serde_json::Value;

use super::{InstallRecord, Source};
use crate::desktops::{Desktop, by_archinstall_name};

const MARKER: &str = "-- Chosen configuration --";

/// Parse the last configuration logged in install.log.
pub fn parse(log: &str, desktops: &[Desktop]) -> Option<InstallRecord> {
    let after = &log[log.rfind(MARKER)? + MARKER.len()..];
    let json_start = after.find("DEBUG - ").map(|i| i + "DEBUG - ".len())?;
    let value = serde_json::Deserializer::from_str(&after[json_start..]).into_iter::<Value>().next()?.ok()?;
    let mut r = from_value(&value, desktops);
    r.installed_at = log.lines().next().and_then(|l| l.strip_prefix('[')?.split(']').next()).map(String::from);
    Some(r)
}

/// Parse a saved `user_configuration.json`.
pub fn parse_config_json(text: &str, desktops: &[Desktop]) -> Option<InstallRecord> {
    let value: Value = serde_json::from_str(text).ok()?;
    value.is_object().then(|| from_value(&value, desktops))
}

fn s(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr).and_then(Value::as_str).map(String::from)
}

fn strings(v: &Value, ptr: &str) -> Vec<String> {
    v.pointer(ptr)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default()
}

fn from_value(v: &Value, desktops: &[Desktop]) -> InstallRecord {
    let apps = v.get("app_config").unwrap_or(v); // pre-3.0 kept audio_config at top level
    let details = strings(v, "/profile_config/profile/details");
    let (zram, zram_algorithm) = match v.get("swap") {
        Some(Value::Bool(b)) => (Some(*b), None),
        Some(o @ Value::Object(_)) => (o.get("enabled").and_then(Value::as_bool), s(o, "/algorithm")),
        _ => (None, None),
    };
    InstallRecord {
        source: Source::ArchinstallLog,
        installer: Some("archinstall".into()),
        archinstall_version: s(v, "/version"),
        profile: s(v, "/profile_config/profile/main"),
        desktops: details.iter().filter_map(|n| by_archinstall_name(desktops, n)).map(|d| d.id.clone()).collect(),
        plasma_flavor: s(v, "/profile_config/profile/custom_settings/KDE Plasma/plasma_flavor"),
        greeter: s(v, "/profile_config/greeter"),
        gfx_driver: s(v, "/profile_config/gfx_driver"),
        kernels: strings(v, "/kernels"),
        bootloader: s(v, "/bootloader_config/bootloader").or_else(|| s(v, "/bootloader")),
        uki: v.pointer("/bootloader_config/uki").and_then(Value::as_bool),
        root_fs: v.get("disk_config").and_then(root_fs),
        encryption: s(v, "/disk_config/disk_encryption/encryption_type"),
        snapshots: s(v, "/disk_config/btrfs_options/snapshot_config/type"),
        zram,
        zram_algorithm,
        network: s(v, "/network_config/type"),
        audio: s(apps, "/audio_config/audio").map(|a| a.to_lowercase()),
        bluetooth: apps.pointer("/bluetooth_config/enabled").and_then(Value::as_bool),
        power: s(apps, "/power_management_config/power_management"),
        printing: apps.pointer("/print_service_config/enabled").and_then(Value::as_bool),
        firewall: s(apps, "/firewall_config/firewall"),
        fonts: strings(apps, "/fonts_config/fonts"),
        multilib: Some(strings(v, "/mirror_config/optional_repositories").iter().any(|r| r == "multilib")),
        pacman_parallel_downloads: v.pointer("/pacman_config/parallel_downloads").and_then(Value::as_u64),
        pacman_color: v.pointer("/pacman_config/color").and_then(Value::as_bool),
        hostname: s(v, "/hostname"),
        timezone: s(v, "/timezone"),
        locale: s(v, "/locale_config/sys_lang"),
        keyboard: s(v, "/locale_config/kb_layout"),
        extra_packages: strings(v, "/packages"),
        services: strings(v, "/services"),
        custom_commands: v.pointer("/custom_commands").and_then(Value::as_array).map_or(0, Vec::len),
        installed_at: None,
    }
}

/// Filesystem of whatever is mounted at `/`, wherever it sits in the layout
/// (plain partition, btrfs subvolume, or LVM volume).
fn root_fs(v: &Value) -> Option<String> {
    match v {
        Value::Object(o) => {
            let fs = o.get("fs_type").and_then(Value::as_str);
            let is_root = o.get("mountpoint").and_then(Value::as_str) == Some("/");
            let root_subvol = o
                .get("btrfs")
                .and_then(Value::as_array)
                .is_some_and(|subs| subs.iter().any(|s| s.get("mountpoint").and_then(Value::as_str) == Some("/")));
            if let Some(fs) = fs
                && (is_root || root_subvol)
            {
                return Some(fs.to_string());
            }
            o.values().find_map(root_fs)
        }
        Value::Array(a) => a.iter().find_map(root_fs),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktops;

    const LOG: &str = r#"[2026-09-10 11:20:01] - INFO - Starting installation...
[2026-09-10 11:20:02] - DEBUG -  -- Chosen configuration --
[2026-09-10 11:20:02] - DEBUG - {
    "app_config": {
        "audio_config": {"audio": "pipewire"},
        "bluetooth_config": {"enabled": true},
        "firewall_config": {"firewall": "ufw"},
        "fonts_config": {"fonts": ["noto-fonts", "noto-fonts-emoji", "noto-fonts-cjk", "ttf-liberation", "ttf-dejavu"]},
        "power_management_config": {"power_management": "tuned"}
    },
    "bootloader_config": {"bootloader": "Systemd-boot", "removable": false, "uki": true},
    "disk_config": {
        "config_type": "default_layout",
        "device_modifications": [{"device": "/dev/nvme0n1", "partitions": [
            {"fs_type": "fat32", "mountpoint": "/boot", "btrfs": []},
            {"fs_type": "btrfs", "mountpoint": null, "btrfs": [{"name": "@", "mountpoint": "/"}]}
        ]}],
        "btrfs_options": {"snapshot_config": {"type": "Snapper"}},
        "disk_encryption": {"encryption_type": "luks"}
    },
    "hostname": "bindle-test",
    "kernels": ["linux-zen"],
    "mirror_config": {"optional_repositories": ["multilib"]},
    "network_config": {"type": "nm"},
    "packages": ["vivaldi"],
    "pacman_config": {"color": true, "parallel_downloads": 8},
    "profile_config": {
        "gfx_driver": "Intel (open-source)",
        "greeter": "plasma-login-manager",
        "profile": {"main": "Desktop", "details": ["KDE Plasma", "Hyprland"],
                    "custom_settings": {"KDE Plasma": {"plasma_flavor": "plasma-meta"}, "Hyprland": {"seat_access": "polkit"}}}
    },
    "swap": {"enabled": true, "algorithm": "zstd"},
    "version": "4.5"
}
[2026-09-10 11:20:03] - INFO - Wiping partitions
"#;

    #[test]
    fn parses_logged_configuration() {
        let d = desktops::all().unwrap();
        let r = parse(LOG, &d).unwrap();
        assert_eq!(r.source, Source::ArchinstallLog);
        assert_eq!(r.desktops, ["plasma", "hyprland"]);
        assert_eq!(r.plasma_flavor.as_deref(), Some("plasma-meta"));
        assert_eq!(r.root_fs.as_deref(), Some("btrfs"));
        assert_eq!(r.encryption.as_deref(), Some("luks"));
        assert_eq!(r.uki, Some(true));
        assert_eq!(r.zram_algorithm.as_deref(), Some("zstd"));
        assert_eq!(r.installed_at.as_deref(), Some("2026-09-10 11:20:01"));
        let provided = r.provided();
        for id in ["audio-pipewire", "bluetooth", "ufw", "fonts", "power-profiles", "multilib", "snapper"] {
            assert!(provided.contains_key(id), "{id}");
        }
    }

    #[test]
    fn takes_the_last_run_and_tolerates_garbage() {
        let d = desktops::all().unwrap();
        assert!(parse("no config here", &d).is_none());
        let twice = format!("{}{}", LOG.replace("bindle-test", "first"), LOG);
        assert_eq!(parse(&twice, &d).unwrap().hostname.as_deref(), Some("bindle-test"));
    }

    #[test]
    fn reads_legacy_bool_swap_and_top_level_audio() {
        let d = desktops::all().unwrap();
        let r = parse_config_json(r#"{"swap": true, "audio_config": {"audio": "Pipewire"}}"#, &d).unwrap();
        assert_eq!(r.zram, Some(true));
        assert_eq!(r.audio.as_deref(), Some("pipewire"));
    }
}
