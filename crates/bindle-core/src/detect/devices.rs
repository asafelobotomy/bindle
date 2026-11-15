//! What the computer is and what's plugged in: the maker and model (DMI),
//! USB and PCI devices, Thunderbolt, monitors, the battery's charge limit.
//! Read from sysfs without root; the tags below come from these facts.

use std::fs;

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Devices {
    /// /sys/class/dmi/id: sys_vendor, product_name, product_version, board_vendor.
    pub sys_vendor: String,
    pub product_name: String,
    pub product_version: String,
    pub board_vendor: String,
    /// USB devices as (vendor, product), and interface classes seen.
    pub usb: Vec<(u16, u16)>,
    pub usb_classes: Vec<u8>,
    /// PCI devices as (vendor, device).
    pub pci: Vec<(u16, u16)>,
    /// A Thunderbolt controller that asks before connecting devices.
    pub thunderbolt: bool,
    /// A monitor on DisplayPort, HDMI or DVI (not the laptop's own panel).
    pub external_monitor: bool,
    /// The built-in panel's dots per inch, from its EDID.
    pub panel_dpi: Option<u32>,
    /// sysfs files that cap the battery's charge (writable as root).
    pub charge_limit: Vec<String>,
}

/// Flag tags these facts give (`known_tag` accepts them).
pub const FLAGS: &[&str] = &[
    "thunderbolt",
    "external-monitor",
    "ipu-camera",
    "fingerprint-reader",
    "security-key",
    "hidpi",
    "handheld",
    "asus-gaming",
    "thinkpad-throttle",
    "charge-limit",
    "pen-tablet",
    "usb-epson",
    "usb-hp",
];

/// `vendor:<maker>` values.
pub const VENDORS: &[&str] = &["asus", "lenovo", "dell", "hp", "framework", "system76", "microsoft", "other"];

/// The maker, from DMI's sys_vendor.
pub fn vendor(sys_vendor: &str) -> &'static str {
    let v = sys_vendor.to_ascii_lowercase();
    let has = |s: &str| v.contains(s);
    if has("asus") {
        "asus"
    } else if has("lenovo") {
        "lenovo"
    } else if has("dell") {
        "dell"
    } else if v.starts_with("hp") || has("hewlett") {
        "hp"
    } else if has("framework") {
        "framework"
    } else if has("system76") {
        "system76"
    } else if has("microsoft") {
        "microsoft"
    } else {
        "other"
    }
}

/// USB fingerprint readers by vendor (Synaptics, Goodix, Validity, Upek,
/// LighTuning/Egis, FocalTech, FPC, AuthenTec), and Elan's 0x0c** readers
/// (Elan's other ids are touchscreens).
pub fn fingerprint_reader(usb: &[(u16, u16)]) -> bool {
    usb.iter().any(|&(v, p)| {
        matches!(v, 0x06cb | 0x27c6 | 0x138a | 0x147e | 0x1c7a | 0x2808 | 0x10a5 | 0x08ff)
            || (v == 0x04f3 && p >> 8 == 0x0c)
    })
}

/// A smartcard reader (CCID class 0x0b) or a FIDO security key maker
/// (Yubico, Nitrokey, Feitian, Token2, SoloKeys).
pub fn security_key(usb: &[(u16, u16)], classes: &[u8]) -> bool {
    classes.contains(&0x0b) || usb.iter().any(|&(v, _)| matches!(v, 0x1050 | 0x20a0 | 0x096e | 0x349e | 0x1209))
}

/// Intel's MIPI camera processing unit (IPU6, IPU7): the webcam works only
/// through libcamera.
pub fn ipu_camera(pci: &[(u16, u16)]) -> bool {
    pci.iter().any(|&(v, d)| {
        v == 0x8086 && matches!(d, 0x9a19 | 0x9a39 | 0x465d | 0x462e | 0xa75d | 0x7d19 | 0x645d | 0xb05d)
    })
}

/// Handheld gaming PCs InputPlumber drives: ROG Ally and Ally X, Legion Go,
/// MSI Claw, Steam Deck (LCD and OLED).
pub fn handheld(product_name: &str) -> bool {
    let p = product_name.trim();
    matches!(p, "RC71L" | "RC72LA" | "83E1" | "Jupiter" | "Galileo") || p.starts_with("Claw")
}

/// ASUS ROG and TUF laptops, where asusctl drives fans, charge limit and lights.
pub fn asus_gaming(d: &Devices) -> bool {
    vendor(&d.sys_vendor) == "asus"
        && ["ROG", "TUF", "Zephyrus", "Strix", "Flow"].iter().any(|n| d.product_name.contains(n))
}

/// ThinkPads with the 8th-generation Intel throttling bug `throttled` fixes.
pub fn thinkpad_throttle(d: &Devices) -> bool {
    vendor(&d.sys_vendor) == "lenovo"
        && ["T480", "T580", "X280", "X1 Carbon 6th", "X1 Yoga 3rd", "L480", "L580", "T470"]
            .iter()
            .any(|m| d.product_version.contains(m))
}

/// A panel's DPI from its EDID: the preferred mode's width (detailed timing
/// at byte 54) over the image width in centimetres (byte 21).
pub fn edid_dpi(edid: &[u8]) -> Option<u32> {
    if edid.len() < 128 || edid[..8] != [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0] {
        return None;
    }
    let cm = u32::from(edid[21]);
    let pixels = u32::from(edid[56]) | (u32::from(edid[58] >> 4) << 8);
    (cm > 0 && pixels > 0).then(|| pixels * 254 / (cm * 100))
}

impl Devices {
    pub fn tags(&self) -> Vec<String> {
        let mut t = vec![format!("vendor:{}", vendor(&self.sys_vendor))];
        for (on, tag) in [
            (self.thunderbolt, "thunderbolt"),
            (self.external_monitor, "external-monitor"),
            (ipu_camera(&self.pci), "ipu-camera"),
            (fingerprint_reader(&self.usb), "fingerprint-reader"),
            (security_key(&self.usb, &self.usb_classes), "security-key"),
            (self.panel_dpi.is_some_and(|d| d >= 180), "hidpi"),
            (handheld(&self.product_name), "handheld"),
            (asus_gaming(self), "asus-gaming"),
            (thinkpad_throttle(self), "thinkpad-throttle"),
            (!self.charge_limit.is_empty(), "charge-limit"),
            // Huion, XP-Pen, UC-Logic tablets (Wacom works in the kernel).
            (self.usb.iter().any(|&(v, _)| matches!(v, 0x256c | 0x28bd | 0x5543)), "pen-tablet"),
            (self.usb.iter().any(|&(v, _)| v == 0x04b8), "usb-epson"),
            (self.usb.iter().any(|&(v, _)| v == 0x03f0), "usb-hp"),
        ] {
            if on {
                t.push(tag.to_string());
            }
        }
        t
    }

    pub fn detect() -> Self {
        let read = |p: &str| fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default();
        let hex = |p: String| u16::from_str_radix(read(&p).trim_start_matches("0x"), 16).ok();
        let dirs = |d: &str| -> Vec<String> {
            fs::read_dir(d).map(|e| e.flatten().map(|e| e.path().display().to_string()).collect()).unwrap_or_default()
        };
        let mut d = Devices {
            sys_vendor: read("/sys/class/dmi/id/sys_vendor"),
            product_name: read("/sys/class/dmi/id/product_name"),
            product_version: read("/sys/class/dmi/id/product_version"),
            board_vendor: read("/sys/class/dmi/id/board_vendor"),
            ..Default::default()
        };
        for dev in dirs("/sys/bus/usb/devices") {
            if let (Some(v), Some(p)) = (hex(format!("{dev}/idVendor")), hex(format!("{dev}/idProduct"))) {
                d.usb.push((v, p));
            }
            if let Ok(c) = u8::from_str_radix(&read(&format!("{dev}/bInterfaceClass")), 16) {
                d.usb_classes.push(c);
            }
        }
        for dev in dirs("/sys/bus/pci/devices") {
            if let (Some(v), Some(id)) = (hex(format!("{dev}/vendor")), hex(format!("{dev}/device"))) {
                d.pci.push((v, id));
            }
        }
        d.thunderbolt = dirs("/sys/bus/thunderbolt/devices")
            .iter()
            .any(|p| matches!(read(&format!("{p}/security")).as_str(), "user" | "secure"));
        for conn in dirs("/sys/class/drm") {
            let name = conn.rsplit('/').next().unwrap_or("");
            let connected = read(&format!("{conn}/status")) == "connected";
            if connected && ["-DP-", "-HDMI-", "-DVI-"].iter().any(|k| name.contains(k)) {
                d.external_monitor = true;
            }
            if connected && name.contains("-eDP-") {
                d.panel_dpi = fs::read(format!("{conn}/edid")).ok().and_then(|e| edid_dpi(&e));
            }
        }
        for bat in dirs("/sys/class/power_supply").iter().filter(|b| b.contains("/BAT")) {
            let f = format!("{bat}/charge_control_end_threshold");
            if std::path::Path::new(&f).exists() {
                d.charge_limit.push(f);
            }
        }
        for p in dirs("/sys/bus/platform/drivers/ideapad_acpi") {
            let f = format!("{p}/conservation_mode");
            if std::path::Path::new(&f).exists() {
                d.charge_limit.push(f);
            }
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makers_and_models() {
        assert_eq!(vendor("ASUSTeK COMPUTER INC."), "asus");
        assert_eq!(vendor("HP"), "hp");
        assert_eq!(vendor("Framework"), "framework");
        assert_eq!(vendor("QEMU"), "other");
        let rog = Devices {
            sys_vendor: "ASUSTeK COMPUTER INC.".into(),
            product_name: "ROG Zephyrus G14 GA402RJ".into(),
            ..Default::default()
        };
        assert!(asus_gaming(&rog) && rog.tags().contains(&"vendor:asus".to_string()));
        let t480 =
            Devices { sys_vendor: "LENOVO".into(), product_version: "ThinkPad T480".into(), ..Default::default() };
        assert!(thinkpad_throttle(&t480));
        assert!(handheld("RC71L") && handheld("Jupiter") && !handheld("ThinkPad"));
    }

    #[test]
    fn devices_by_id() {
        assert!(fingerprint_reader(&[(0x27c6, 0x55a4)]) && fingerprint_reader(&[(0x04f3, 0x0c4b)]));
        assert!(!fingerprint_reader(&[(0x04f3, 0x2a1c)]), "an Elan touchscreen");
        assert!(security_key(&[(0x1050, 0x0407)], &[]) && security_key(&[], &[0x0b]) && !security_key(&[], &[3]));
        assert!(ipu_camera(&[(0x8086, 0x465d)]) && !ipu_camera(&[(0x8086, 0x9a49)]));
    }

    #[test]
    fn panel_dpi_from_edid() {
        let mut e = vec![0u8; 128];
        e[..8].copy_from_slice(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
        e[21] = 29; // 29 cm wide
        let width: u32 = 2880;
        e[56] = (width & 0xff) as u8;
        e[58] = ((width >> 8) as u8) << 4;
        assert_eq!(edid_dpi(&e), Some(252));
        e[21] = 0;
        assert_eq!(edid_dpi(&e), None, "projectors give no size");
        assert_eq!(edid_dpi(&[0; 10]), None);
    }
}
