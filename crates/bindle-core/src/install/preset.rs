//! Pre-filled archinstall configuration for `bindle iso`. archinstall opens
//! its normal menu with these values set; disks, users, hostname and locale
//! are still chosen there.

use serde_json::{Value, json};

use super::GFX_DRIVERS;
use crate::builtins::nvidia_open_supported;
use crate::desktops::{Desktop, by_id};
use crate::detect::Facts;
use crate::plan::Selection;

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
pub fn gfx_driver(facts: &Facts) -> Option<&'static str> {
    let has = |v: &str| facts.gpus.iter().any(|g| g == v);
    let idx = if has("nvidia") && nvidia_open_supported(facts) {
        3 // open kernel module: Turing and newer
    } else if has("nvidia") {
        // nvidia-open can't drive pre-Turing cards (they need the AUR's
        // nvidia-580xx-dkms): nouveau, plus the iGPU's drivers on hybrids.
        if has("amd") || has("intel") { 0 } else { 4 }
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
    if let Some(chosen) = desktop {
        // uQ has no archinstall profile: install its base's, with uQ's greeter.
        let d = chosen.profile(desktops);
        let mut custom = json!({});
        if d.id == "plasma" {
            custom = json!({ "plasma_flavor": "plasma-meta" });
        } else if ["hyprland", "sway", "labwc", "niri"].contains(&d.id.as_str()) {
            custom = json!({ "seat_access": "systemd-logind" });
        }
        let mut profile = json!({
            "profile": { "main": "Desktop", "details": [d.name], "custom_settings": { d.name.clone(): custom } },
        });
        if let Some(g) = chosen.greeter.as_ref().or(d.greeter.as_ref()) {
            profile["greeter"] = json!(g);
        }
        if let Some(gfx) = gfx_driver(facts) {
            profile["gfx_driver"] = json!(gfx);
        }
        cfg["profile_config"] = profile;
    }
    cfg
}

/// The selection Bindle opens with on first login: the profile, plus the
/// module that installs a desktop archinstall can't (uQ, from the AUR) and
/// that desktop's own apps.
pub fn first_login_selection(desktops: &[Desktop], choice: &IsoChoice) -> Selection {
    let desktop = choice.desktop.as_deref().and_then(|id| by_id(desktops, id));
    let include = desktop.map(|d| d.module.iter().chain(&d.apps).cloned().collect()).unwrap_or_default();
    Selection { profile: Some(choice.profile.clone()), include, ..Default::default() }
}

/// Bindle's preset for `choice` with the user's `--config` overlay merged in.
pub fn build(facts: &Facts, desktops: &[Desktop], choice: &IsoChoice, overlay: Option<&Value>) -> Value {
    let mut cfg = archinstall_preset(facts, desktops, choice);
    if let Some(o) = overlay {
        merge(&mut cfg, o.clone());
    }
    cfg
}

const OPTIONAL_REPOS: &str = "/mirror_config/optional_repositories";

/// Deep-merge `overlay` into `base`; overlay values win, objects merge.
/// `mirror_config.optional_repositories` is the union of both lists, so a
/// user config that adds a repo doesn't drop the one a profile needs.
pub fn merge(base: &mut Value, overlay: Value) {
    let ours = base.pointer(OPTIONAL_REPOS).and_then(Value::as_array).cloned();
    deep_merge(base, overlay);
    if let (Some(ours), Some(Value::Array(merged))) = (ours, base.pointer_mut(OPTIONAL_REPOS)) {
        for repo in ours {
            if !merged.contains(&repo) {
                merged.push(repo);
            }
        }
    }
}

fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(slot) => deep_merge(slot, v),
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
    out.push(format!("Audio        {}", s("/app_config/audio_config/audio").unwrap_or("choose in archinstall".into())));
    out.push("Network      NetworkManager".into());
    out.push("Swap         zram (zstd)".into());
    out.push("pacman       colour, 10 parallel downloads".into());
    if preset.pointer("/app_config/firewall_config").is_some() {
        out.push(format!("Firewall     {}", s("/app_config/firewall_config/firewall").unwrap_or("ufw".into())));
        out.push("Fonts        Noto (+emoji, CJK), Liberation, DejaVu".into());
    }
    if preset.pointer("/app_config/bluetooth_config").is_some() {
        out.push("Bluetooth    enabled".into());
    }
    if preset.pointer("/app_config/power_management_config").is_some() {
        out.push("Power        power-profiles-daemon".into());
    }
    let repos: Vec<String> = preset
        .pointer(OPTIONAL_REPOS)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|r| format!("[{r}]"))
        .collect();
    if !repos.is_empty() {
        out.push(format!("Repos        {}", repos.join(" ")));
    }
    if preset.pointer("/disk_config").is_some() {
        out.push("Disks        from --config".into());
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
    fn uq_installs_hyprlands_profile_with_its_own_greeter() {
        let d = desktops::all().unwrap();
        let choice = IsoChoice { desktop: Some("uq".into()), profile: "desktop".into() };
        let preset = archinstall_preset(&Facts::default(), &d, &choice);
        assert_eq!(preset.pointer("/profile_config/profile/details/0").unwrap(), "Hyprland");
        assert_eq!(
            preset.pointer("/profile_config/profile/custom_settings/Hyprland/seat_access").unwrap(),
            "systemd-logind"
        );
        let r = archinstall_log::parse_config_json(&preset.to_string(), &d).unwrap();
        assert_eq!(r.desktops, ["hyprland"]);
        assert_eq!(r.greeter.as_deref(), Some("sddm"));
    }

    #[test]
    fn uq_is_finished_by_its_module_on_first_login() {
        let data = desktops::all().unwrap();
        let pick = |d: Option<&str>| IsoChoice { desktop: d.map(String::from), profile: "desktop".into() };
        let sel = first_login_selection(&data, &pick(Some("uq")));
        let include = ["uq", "scdr", "crumbs", "noti-fi", "text-editor", "marknote"].map(String::from).to_vec();
        assert_eq!((sel.profile.as_deref(), sel.include), (Some("desktop"), include));
        assert!(first_login_selection(&data, &pick(Some("plasma"))).include.is_empty());
        assert!(first_login_selection(&data, &pick(None)).include.is_empty());
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
    fn optional_repositories_are_combined_not_replaced() {
        let d = desktops::all().unwrap();
        let choice = IsoChoice { desktop: None, profile: "gaming".into() };
        let overlay = json!({"mirror_config": {"optional_repositories": ["testing", "multilib"]}});
        let cfg = build(&Facts::default(), &d, &choice, Some(&overlay));
        assert_eq!(cfg.pointer(OPTIONAL_REPOS).unwrap(), &json!(["testing", "multilib"]));
        let overlay = json!({"mirror_config": {"optional_repositories": ["testing"]}});
        let cfg = build(&Facts::default(), &d, &choice, Some(&overlay));
        assert_eq!(cfg.pointer(OPTIONAL_REPOS).unwrap(), &json!(["testing", "multilib"]));
        assert!(summary(&cfg).contains(&"Repos        [testing] [multilib]".to_string()));
    }

    #[test]
    fn summary_reflects_the_merged_config() {
        let d = desktops::all().unwrap();
        let choice = IsoChoice { desktop: None, profile: "desktop".into() };
        let plain = summary(&build(&Facts::default(), &d, &choice, None));
        assert!(!plain.iter().any(|l| l.starts_with("Repos") || l.starts_with("Disks")));
        let overlay = json!({
            "mirror_config": {"optional_repositories": ["multilib"]},
            "app_config": {"firewall_config": {"firewall": "firewalld"}},
            "disk_config": {"device_modifications": []},
        });
        let merged = summary(&build(&Facts::default(), &d, &choice, Some(&overlay)));
        assert!(merged.contains(&"Repos        [multilib]".to_string()));
        assert!(merged.contains(&"Firewall     firewalld".to_string()));
        assert!(merged.contains(&"Disks        from --config".to_string()));
    }

    #[test]
    fn gfx_mapping() {
        let gpus = |g: &[&str], nvidia: &[u16]| Facts {
            gpus: g.iter().map(|s| s.to_string()).collect(),
            nvidia_devices: nvidia.to_vec(),
            ..Default::default()
        };
        assert_eq!(gfx_driver(&gpus(&["intel", "nvidia"], &[0x1f08])), Some(GFX_DRIVERS[3].0)); // RTX 2060
        assert_eq!(gfx_driver(&gpus(&["nvidia"], &[])), Some(GFX_DRIVERS[3].0)); // unknown id: assume current
        // Pre-Turing (GTX 1060 Mobile): never nvidia-open.
        assert_eq!(gfx_driver(&gpus(&["intel", "nvidia"], &[0x1c20])), Some("All open-source"));
        assert_eq!(gfx_driver(&gpus(&["nvidia"], &[0x1c20])), Some(GFX_DRIVERS[4].0));
        assert_eq!(gfx_driver(&gpus(&["intel", "amd"], &[])), Some("All open-source"));
        assert_eq!(gfx_driver(&gpus(&[], &[])), None);
    }
}
