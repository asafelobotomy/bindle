//! The hardware-dependent settings `bindle install` recommends and lets you
//! change on its Hardware page: one row per setting, the detected choice
//! first selected. Pure: `recommended` reads `Facts`, `apply` writes the
//! chosen values into an archinstall config.

use serde_json::{Value, json};

use super::GFX_DRIVERS;
use super::preset::gfx_driver;
use crate::builtins::nvidia_legacy;
use crate::detect::Facts;

/// The module that installs NVIDIA's 580 driver for older cards.
pub const NVIDIA_LEGACY_MODULE: &str = "nvidia-legacy";

/// Plain names for archinstall's driver choices (`GFX_DRIVERS` order): which
/// is the maker's own driver and which the open-source one.
const GFX_LABELS: [&str; 6] = [
    "All open-source drivers",
    "AMD (open-source, AMD's own)",
    "Intel (open-source, Intel's own)",
    "NVIDIA's own driver (RTX and GTX 16 cards)",
    "NVIDIA open-source driver (nouveau)",
    "VirtualBox",
];

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
    /// A Bindle module this choice stands for: installed with it, left out
    /// with any other choice of the row.
    pub module: Option<&'static str>,
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
    Choice { label, value, module: None }
}

/// The graphics row: the maker's own driver recommended wherever one can be
/// installed, the open-source driver as the other choice.
fn graphics_row(facts: &Facts, gpus: &str) -> Row {
    let mut pick = gfx_driver(facts).and_then(|name| GFX_DRIVERS.iter().position(|d| d.0 == name));
    let mut choices: Vec<Choice> = GFX_DRIVERS.iter().zip(GFX_LABELS).map(|(d, label)| c(label, d.0)).collect();
    let nvidia = facts.gpus.iter().any(|g| g == "nvidia");
    let why = if nvidia_legacy(facts) {
        // An older card: NVIDIA's own driver comes from the AUR, on top of
        // what archinstall installs for the open-source choice.
        let legacy = Choice {
            label: "NVIDIA's own driver for older cards (580xx, built from the AUR)",
            value: GFX_DRIVERS[pick.unwrap_or(0)].0,
            module: Some(NVIDIA_LEGACY_MODULE),
        };
        choices.insert(0, legacy);
        pick = Some(0);
        format!(
            "Detected graphics: {gpus}, an older NVIDIA card. NVIDIA's own driver is recommended: \
             full speed and video decoding. The open-source nouveau driver is the other choice."
        )
    } else if nvidia && pick.is_some_and(|i| GFX_DRIVERS[i].0.starts_with("Nvidia (open kernel")) {
        format!(
            "Detected graphics: {gpus}. NVIDIA's own driver is recommended; the open-source nouveau driver is the other choice."
        )
    } else if nvidia {
        format!(
            "Detected graphics: {gpus}. NVIDIA no longer makes a driver Bindle can install for this card, \
             so the open-source one is used."
        )
    } else if pick.is_some() {
        format!("Detected graphics: {gpus}.")
    } else {
        format!("Detected graphics: {gpus}. The open-source drivers cover every card.")
    };
    row(Setting::Graphics, "Graphics driver", choices, pick.unwrap_or(0), why)
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
            rows.push(graphics_row(facts, &gpus));
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

    /// Modules a row offers that its selected choice doesn't stand for: left
    /// out of the install (the open-source driver was chosen instead).
    pub fn excluded_modules(&self) -> Vec<&'static str> {
        self.rows
            .iter()
            .flat_map(|r| r.choices.iter().filter_map(|c| c.module).filter(|m| r.choice().module != Some(*m)))
            .collect()
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
#[path = "hardware_tests.rs"]
mod tests;
