//! The hardware-dependent settings `bindle install` recommends and lets you
//! change on its Hardware page: one row per setting, the detected choice
//! first selected. Pure: `recommended` reads `Facts`, `apply` writes the
//! chosen values into an archinstall config.

use serde_json::{Value, json};

use super::GFX_DRIVERS;
use super::preset::gfx_driver;
use crate::detect::Facts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Graphics,
    Microcode,
    Kernel,
    Bootloader,
    Swap,
    Power,
    Bluetooth,
    Audio,
}

/// One choice of a row: what the page shows and what archinstall is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub label: &'static str,
    /// archinstall's value; empty means "leave it out".
    pub value: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub setting: Setting,
    pub label: &'static str,
    pub choices: Vec<Choice>,
    pub selected: usize,
    /// The choice detection picked, marked on the page.
    pub recommended: usize,
    /// Why that one, in a sentence.
    pub why: String,
}

impl Row {
    pub fn choice(&self) -> &Choice {
        &self.choices[self.selected.min(self.choices.len() - 1)]
    }

    /// One choice only: shown, not changeable.
    pub fn fixed(&self) -> bool {
        self.choices.len() < 2
    }

    pub fn cycle(&mut self, forward: bool) {
        let n = self.choices.len();
        self.selected = (self.selected + if forward { 1 } else { n - 1 }) % n;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hardware {
    pub rows: Vec<Row>,
}

fn c(label: &'static str, value: &'static str) -> Choice {
    Choice { label, value }
}

fn row(setting: Setting, label: &'static str, choices: Vec<Choice>, pick: usize, why: impl Into<String>) -> Row {
    Row { setting, label, choices, selected: pick, recommended: pick, why: why.into() }
}

const OFF: &str = "";

impl Hardware {
    /// The rows for this machine. `graphical`: a desktop is being installed
    /// (the graphics driver, power profiles and Bluetooth only apply then).
    pub fn recommended(facts: &Facts, uefi: bool, graphical: bool) -> Self {
        let mut rows = Vec::new();
        if graphical {
            let gpus = if facts.gpus.is_empty() { "no GPU detected".to_string() } else { facts.gpus.join(" + ") };
            let pick = gfx_driver(facts).and_then(|name| GFX_DRIVERS.iter().position(|d| d.0 == name));
            let choices = GFX_DRIVERS.iter().map(|d| c(d.0, d.0)).collect();
            let why = match pick {
                Some(_) => format!("Detected graphics: {gpus}."),
                None => format!("Detected graphics: {gpus}. The open-source drivers cover every card."),
            };
            rows.push(row(Setting::Graphics, "Graphics driver", choices, pick.unwrap_or(0), why));
        }
        let ucode = match facts.cpu_vendor.to_ascii_lowercase() {
            v if v.contains("intel") => Some("intel-ucode"),
            v if v.contains("amd") => Some("amd-ucode"),
            _ => None,
        };
        let ucode_row = match ucode {
            Some(p) => row(Setting::Microcode, "CPU microcode", vec![c(p, OFF)], 0, "Installed for the detected CPU."),
            None => {
                row(Setting::Microcode, "CPU microcode", vec![c("none needed", OFF)], 0, "Not an Intel or AMD CPU.")
            }
        };
        rows.push(ucode_row);
        rows.push(row(
            Setting::Kernel,
            "Kernel",
            vec![
                c("linux (Arch's standard)", "linux"),
                c("linux-lts (long-term)", "linux-lts"),
                c("linux-zen", "linux-zen"),
            ],
            0,
            "The standard kernel has every driver and the newest fixes.",
        ));
        let (boot, why) = if uefi {
            (
                vec![c("systemd-boot", "Systemd-boot"), c("GRUB", "Grub"), c("Limine", "Limine")],
                "This computer starts with UEFI: systemd-boot is the simplest loader for it.",
            )
        } else {
            (
                vec![c("GRUB", "Grub"), c("Limine", "Limine")],
                "This computer starts with BIOS, which systemd-boot can't do.",
            )
        };
        rows.push(row(Setting::Bootloader, "Boot loader", boot, 0, why));
        rows.push(row(
            Setting::Swap,
            "Swap",
            vec![c("zram (compressed, in memory)", "zstd"), c("none", OFF)],
            0,
            format!(
                "{:.0} GiB of memory: zram gives headroom without a swap partition.",
                facts.ram_mib as f64 / 1024.0
            ),
        ));
        if graphical {
            let power = vec![c("power-profiles-daemon", "power-profiles-daemon"), c("TuneD", "tuned"), c("none", OFF)];
            let (pick, why) = if facts.laptop {
                (0, "A laptop: power profiles (saver, balanced, performance) in the desktop's menu.")
            } else {
                (2, "Not a laptop: nothing to save battery for.")
            };
            rows.push(row(Setting::Power, "Power profiles", power, pick, why));
            let (pick, why) = if facts.bluetooth {
                (0, "A Bluetooth adapter was found.")
            } else {
                (1, "No Bluetooth adapter was found.")
            };
            rows.push(row(Setting::Bluetooth, "Bluetooth", vec![c("on", "on"), c("off", OFF)], pick, why));
        }
        rows.push(row(
            Setting::Audio,
            "Audio",
            vec![c("PipeWire", "pipewire"), c("PulseAudio", "pulseaudio"), c("none", "No audio server")],
            0,
            "PipeWire is what current desktops and apps expect.",
        ));
        Hardware { rows }
    }

    fn value(&self, s: Setting) -> Option<&'static str> {
        self.rows.iter().find(|r| r.setting == s).map(|r| r.choice().value)
    }

    /// True once any row differs from the recommendation.
    pub fn changed(&self) -> bool {
        self.rows.iter().any(|r| r.selected != r.recommended)
    }

    /// Write the chosen values over `cfg` (Bindle's preset for the install).
    pub fn apply(&self, cfg: &mut Value) {
        if let Some(k) = self.value(Setting::Kernel) {
            cfg["kernels"] = json!([k]);
        }
        if let Some(b) = self.value(Setting::Bootloader) {
            cfg["bootloader_config"] = json!({ "bootloader": b, "uki": false, "removable": false });
        }
        if let Some(algo) = self.value(Setting::Swap) {
            cfg["swap"] =
                if algo == OFF { json!({ "enabled": false }) } else { json!({ "enabled": true, "algorithm": algo }) };
        }
        if let Some(a) = self.value(Setting::Audio) {
            cfg["app_config"]["audio_config"] = json!({ "audio": a });
        }
        let apps = cfg["app_config"].as_object_mut();
        if let (Some(apps), Some(p)) = (apps, self.value(Setting::Power)) {
            if p == OFF {
                apps.remove("power_management_config");
            } else {
                apps.insert("power_management_config".into(), json!({ "power_management": p }));
            }
        }
        if let (Some(apps), Some(b)) = (cfg["app_config"].as_object_mut(), self.value(Setting::Bluetooth)) {
            if b == OFF {
                apps.remove("bluetooth_config");
            } else {
                apps.insert("bluetooth_config".into(), json!({ "enabled": true }));
            }
        }
        // Only a desktop install has a profile to hang the driver on.
        if let (Some(g), Some(profile)) = (self.value(Setting::Graphics), cfg.get_mut("profile_config")) {
            profile["gfx_driver"] = json!(g);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktops;
    use crate::install::preset::{self, IsoChoice};

    fn laptop() -> Facts {
        Facts {
            gpus: vec!["intel".into()],
            cpu_vendor: "GenuineIntel".into(),
            laptop: true,
            bluetooth: true,
            ram_mib: 16384,
            ..Default::default()
        }
    }

    fn pick(hw: &mut Hardware, s: Setting, value: &str) {
        let r = hw.rows.iter_mut().find(|r| r.setting == s).unwrap();
        r.selected = r.choices.iter().position(|c| c.value == value).unwrap();
    }

    #[test]
    fn recommendation_matches_the_preset() {
        let d = desktops::all().unwrap();
        let choice = IsoChoice { desktop: Some("uq".into()), profile: "desktop".into() };
        let before = preset::build(&laptop(), &d, &choice, None);
        let mut after = before.clone();
        let hw = Hardware::recommended(&laptop(), true, true);
        assert!(!hw.changed());
        hw.apply(&mut after);
        // Nothing the preset decided changes; kernel and boot loader are added.
        for key in ["app_config", "swap", "profile_config"] {
            assert_eq!(before[key], after[key], "{key}");
        }
        assert_eq!(after["kernels"], json!(["linux"]));
        assert_eq!(after["bootloader_config"]["bootloader"], "Systemd-boot");
        let ucode = hw.rows.iter().find(|r| r.setting == Setting::Microcode).unwrap();
        assert!(ucode.fixed() && ucode.choice().label == "intel-ucode");
    }

    #[test]
    fn changed_rows_reach_the_config() {
        let d = desktops::all().unwrap();
        let choice = IsoChoice { desktop: Some("plasma".into()), profile: "desktop".into() };
        let mut cfg = preset::build(&laptop(), &d, &choice, None);
        let mut hw = Hardware::recommended(&laptop(), true, true);
        pick(&mut hw, Setting::Kernel, "linux-lts");
        pick(&mut hw, Setting::Bootloader, "Grub");
        pick(&mut hw, Setting::Swap, OFF);
        pick(&mut hw, Setting::Power, OFF);
        pick(&mut hw, Setting::Bluetooth, OFF);
        pick(&mut hw, Setting::Audio, "pulseaudio");
        pick(&mut hw, Setting::Graphics, GFX_DRIVERS[0].0);
        assert!(hw.changed());
        hw.apply(&mut cfg);
        assert_eq!(cfg["kernels"], json!(["linux-lts"]));
        assert_eq!(cfg["bootloader_config"]["bootloader"], "Grub");
        assert_eq!(cfg["swap"], json!({ "enabled": false }));
        assert!(cfg["app_config"].get("power_management_config").is_none());
        assert!(cfg["app_config"].get("bluetooth_config").is_none());
        assert_eq!(cfg["app_config"]["audio_config"]["audio"], "pulseaudio");
        assert_eq!(cfg["profile_config"]["gfx_driver"], "All open-source");
    }

    #[test]
    fn bios_and_no_desktop() {
        let hw = Hardware::recommended(&Facts::default(), false, false);
        let has = |s| hw.rows.iter().any(|r| r.setting == s);
        assert!(!has(Setting::Graphics) && !has(Setting::Power) && !has(Setting::Bluetooth));
        let boot = hw.rows.iter().find(|r| r.setting == Setting::Bootloader).unwrap();
        assert_eq!(boot.choice().value, "Grub");
        assert!(boot.choices.iter().all(|c| c.value != "Systemd-boot"));
        let d = desktops::all().unwrap();
        let mut cfg =
            preset::build(&Facts::default(), &d, &IsoChoice { desktop: None, profile: "minimal".into() }, None);
        hw.apply(&mut cfg);
        assert!(cfg.get("profile_config").is_none());
    }

    #[test]
    fn rows_cycle_both_ways() {
        let mut hw = Hardware::recommended(&laptop(), true, true);
        let k = hw.rows.iter_mut().find(|r| r.setting == Setting::Kernel).unwrap();
        k.cycle(false);
        assert_eq!(k.choice().value, "linux-zen");
        k.cycle(true);
        assert_eq!(k.choice().value, "linux");
    }
}
