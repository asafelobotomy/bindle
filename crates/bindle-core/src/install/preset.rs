//! Pre-filled archinstall configuration for `bindle iso`. archinstall opens
//! its normal menu with these values set; disks, users, hostname and locale
//! are still chosen there.

use serde_json::{Value, json};

use super::GFX_DRIVERS;
use crate::desktops::{Desktop, by_id};
use crate::detect::Facts;

/// archinstall release whose config schema and plugin API this was built against.
pub const TESTED_ARCHINSTALL: &str = "4.5";

#[derive(Debug, Clone)]
pub struct IsoChoice {
    /// Bindle desktop id, or None to pick (or skip) a desktop in archinstall.
    pub desktop: Option<String>,
    /// Bindle profile applied on first login.
    pub profile: String,
}

/// archinstall's graphics-driver choice for the detected GPUs.
pub fn gfx_driver(gpus: &[String]) -> Option<&'static str> {
    let has = |v: &str| gpus.iter().any(|g| g == v);
    let idx = if has("nvidia") {
        3 // open kernel module; pre-Turing cards need an AUR legacy driver
    } else if has("amd") && has("intel") {
        0
    } else if has("amd") {
        1
    } else if has("intel") {
        2
    } else if has("virtual") {
        5
    } else {
        return None;
    };
    Some(GFX_DRIVERS[idx].0)
}

pub fn archinstall_preset(facts: &Facts, desktops: &[Desktop], choice: &IsoChoice) -> Value {
    let desktop = choice.desktop.as_deref().and_then(|id| by_id(desktops, id));
    let graphical = desktop.is_some() || choice.profile != "minimal";
    let mut apps = json!({ "audio_config": { "audio": "pipewire" } });
    if graphical {
        apps["fonts_config"] =
            json!({ "fonts": ["noto-fonts", "noto-fonts-emoji", "noto-fonts-cjk", "ttf-liberation", "ttf-dejavu"] });
        apps["firewall_config"] = json!({ "firewall": "ufw" });
        if facts.bluetooth {
            apps["bluetooth_config"] = json!({ "enabled": true });
        }
        if facts.laptop {
            apps["power_management_config"] = json!({ "power_management": "power-profiles-daemon" });
        }
    }
    let mut cfg = json!({
        "app_config": apps,
        // Desktop network panels and applets expect NetworkManager.
        "network_config": { "type": "nm" },
        "swap": { "enabled": true, "algorithm": "zstd" },
        "pacman_config": { "color": true, "parallel_downloads": 10 },
        "ntp": true,
    });
    if choice.profile == "gaming" {
        cfg["mirror_config"] = json!({ "optional_repositories": ["multilib"] });
    }
    if let Some(d) = desktop {
        let mut custom = json!({});
        if d.id == "plasma" {
            custom = json!({ "plasma_flavor": "plasma-meta" });
        } else if ["hyprland", "sway", "labwc", "niri"].contains(&d.id.as_str()) {
            custom = json!({ "seat_access": "systemd-logind" });
        }
        let mut profile = json!({
            "profile": { "main": "Desktop", "details": [d.name], "custom_settings": { d.name.clone(): custom } },
        });
        if let Some(g) = &d.greeter {
            profile["greeter"] = json!(g);
        }
        if let Some(gfx) = gfx_driver(&facts.gpus) {
            profile["gfx_driver"] = json!(gfx);
        }
        cfg["profile_config"] = profile;
    }
    cfg
}

/// Deep-merge `overlay` into `base`; overlay values win, objects merge.
pub fn merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(slot) => merge(slot, v),
                    None => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (slot, v) => *slot = v,
    }
}

/// Human summary of what the preset sets, for the pre-install screen.
pub fn summary(preset: &Value) -> Vec<String> {
    let s = |p: &str| preset.pointer(p).and_then(Value::as_str).map(String::from);
    let mut out = Vec::new();
    if let Some(d) = preset.pointer("/profile_config/profile/details/0").and_then(Value::as_str) {
        out.push(format!("Desktop      {d} · greeter {}", s("/profile_config/greeter").unwrap_or("default".into())));
        out.push(format!("Graphics     {}", s("/profile_config/gfx_driver").unwrap_or("choose in archinstall".into())));
    } else {
        out.push("Desktop      choose in archinstall (or none)".into());
    }
    out.push("Audio        PipeWire".into());
    out.push("Network      NetworkManager".into());
    out.push("Swap         zram (zstd)".into());
    out.push("pacman       colour, 10 parallel downloads".into());
    if preset.pointer("/app_config/firewall_config").is_some() {
        out.push("Firewall     ufw".into());
        out.push("Fonts        Noto (+emoji, CJK), Liberation, DejaVu".into());
    }
    if preset.pointer("/app_config/bluetooth_config").is_some() {
        out.push("Bluetooth    enabled".into());
    }
    if preset.pointer("/app_config/power_management_config").is_some() {
        out.push("Power        power-profiles-daemon".into());
    }
    if preset.pointer("/mirror_config").is_some() {
        out.push("Repos        [multilib]".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktops;
    use crate::install::archinstall_log;

    #[test]
    fn preset_round_trips_through_our_archinstall_parser() {
        let d = desktops::all().unwrap();
        let facts = Facts { gpus: vec!["amd".into()], laptop: true, bluetooth: true, ..Default::default() };
        let choice = IsoChoice { desktop: Some("plasma".into()), profile: "gaming".into() };
        let preset = archinstall_preset(&facts, &d, &choice);
        let r = archinstall_log::parse_config_json(&preset.to_string(), &d).unwrap();
        assert_eq!(r.desktops, ["plasma"]);
        assert_eq!(r.greeter.as_deref(), Some("plasma-login-manager"));
        assert_eq!(r.gfx_driver.as_deref(), Some("AMD / ATI (open-source)"));
        assert_eq!(r.power.as_deref(), Some("power-profiles-daemon"));
        assert_eq!(r.multilib, Some(true));
        assert_eq!(r.network.as_deref(), Some("nm"));
        assert_eq!(r.plasma_flavor.as_deref(), Some("plasma-meta"));
    }

    #[test]
    fn minimal_without_desktop_stays_minimal() {
        let d = desktops::all().unwrap();
        let facts = Facts { bluetooth: true, laptop: true, ..Default::default() };
        let preset = archinstall_preset(&facts, &d, &IsoChoice { desktop: None, profile: "minimal".into() });
        assert!(preset.get("profile_config").is_none());
        assert!(preset.pointer("/app_config/bluetooth_config").is_none());
        assert!(preset.pointer("/app_config/audio_config").is_some());
    }

    #[test]
    fn overlay_wins_and_objects_merge() {
        let mut base = json!({"app_config": {"audio_config": {"audio": "pipewire"}}, "ntp": true});
        merge(
            &mut base,
            json!({"app_config": {"firewall_config": {"firewall": "firewalld"}}, "ntp": false, "hostname": "x"}),
        );
        assert_eq!(base["app_config"]["audio_config"]["audio"], "pipewire");
        assert_eq!(base["app_config"]["firewall_config"]["firewall"], "firewalld");
        assert_eq!(base["ntp"], false);
        assert_eq!(base["hostname"], "x");
    }

    #[test]
    fn gfx_mapping() {
        assert_eq!(gfx_driver(&["intel".into(), "nvidia".into()]), Some(GFX_DRIVERS[3].0));
        assert_eq!(gfx_driver(&["intel".into(), "amd".into()]), Some("All open-source"));
        assert_eq!(gfx_driver(&[]), None);
    }
}
