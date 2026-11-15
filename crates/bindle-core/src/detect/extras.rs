//! Smaller facts some modules need: Wi-Fi, the country (for Wi-Fi's
//! regulatory domain), and the Intel GPU's device ids (which video-decoding
//! driver it needs).

use std::fs;

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Extras {
    /// A Wi-Fi device (`/sys/class/ieee80211/*`).
    pub wifi: bool,
    /// ISO country code from the timezone (`zone.tab`), e.g. "GB".
    pub country: Option<String>,
    /// The timezone's city as (latitude, longitude), for night light.
    pub location: Option<(f64, f64)>,
    /// PCI device ids of Intel GPUs.
    pub intel_devices: Vec<u16>,
}

/// The country a timezone is in, from zone.tab's `CC<TAB>coords<TAB>Zone` lines.
pub fn country(zone: &str, zone_tab: &str) -> Option<String> {
    zone_tab.lines().filter(|l| !l.starts_with('#')).find_map(|l| {
        let mut f = l.split('\t');
        let (cc, _, z) = (f.next()?, f.next()?, f.next()?);
        (z == zone && cc.len() == 2).then(|| cc.to_ascii_uppercase())
    })
}

/// A timezone's city as (latitude, longitude) from zone.tab's coordinates
/// (`+DDMM+DDDMM` or `+DDMMSS+DDDMMSS`).
pub fn location(zone: &str, zone_tab: &str) -> Option<(f64, f64)> {
    let coords = zone_tab.lines().filter(|l| !l.starts_with('#')).find_map(|l| {
        let f: Vec<&str> = l.split('\t').collect();
        (f.get(2) == Some(&zone)).then(|| f.get(1).map(|c| c.to_string())).flatten()
    })?;
    let split = coords[1..].find(['+', '-'])? + 1;
    let part = |s: &str, deg: usize| -> Option<f64> {
        let sign = if s.starts_with('-') { -1.0 } else { 1.0 };
        let d = &s[1..];
        let num = |r: std::ops::Range<usize>| d.get(r).and_then(|x| x.parse::<f64>().ok());
        let value = num(0..deg)? + num(deg..deg + 2)? / 60.0 + num(deg + 2..deg + 4).unwrap_or(0.0) / 3600.0;
        Some(sign * value)
    };
    Some((part(&coords[..split], 2)?, part(&coords[split..], 3)?))
}

/// The zone /etc/localtime points at ("Europe/London").
fn zone() -> Option<String> {
    let target = fs::read_link("/etc/localtime").ok()?;
    let s = target.to_string_lossy();
    s.split_once("zoneinfo/").map(|(_, z)| z.to_string())
}

impl Extras {
    pub fn detect(intel_devices: Vec<u16>) -> Self {
        let wifi = fs::read_dir("/sys/class/ieee80211").is_ok_and(|mut d| d.next().is_some());
        let zone_tab = fs::read_to_string("/usr/share/zoneinfo/zone.tab").unwrap_or_default();
        let zone = zone();
        let country = zone.as_deref().and_then(|z| country(z, &zone_tab));
        let location = zone.as_deref().and_then(|z| location(z, &zone_tab));
        Self { wifi, country, location, intel_devices }
    }
}

/// Intel GPUs `intel-media-driver` (iHD) doesn't support: Haswell and older
/// (Gen 7.5 and before), which need `libva-intel-driver` (i965). By PCI
/// device id: Ironlake 0x004x, Sandy/Ivy Bridge 0x01xx, Haswell 0x04xx,
/// 0x0Axx, 0x0Cxx, 0x0Dxx, Bay Trail 0x0Fxx.
pub fn intel_gen_old(device: u16) -> bool {
    matches!(device >> 8, 0x01 | 0x04 | 0x0a | 0x0c | 0x0d | 0x0f) || device >> 4 == 0x004
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_from_zone() {
        let tab = "# comment\nGB\t+513030-0000731\tEurope/London\nDE\t+5230+01322\tEurope/Berlin\tmost of Germany\n";
        assert_eq!(country("Europe/London", tab).as_deref(), Some("GB"));
        assert_eq!(country("Europe/Berlin", tab).as_deref(), Some("DE"));
        assert_eq!(country("UTC", tab), None);
        let (lat, lon) = location("Europe/London", tab).unwrap();
        assert!((lat - 51.508).abs() < 0.01 && (lon + 0.125).abs() < 0.01, "{lat} {lon}");
        let (lat, lon) = location("Europe/Berlin", tab).unwrap();
        assert!((lat - 52.5).abs() < 0.01 && (lon - 13.367).abs() < 0.01, "{lat} {lon}");
    }

    #[test]
    fn old_intel_generations() {
        for old in [0x0046, 0x0116, 0x0166, 0x0412, 0x0a16, 0x0d26, 0x0f31] {
            assert!(intel_gen_old(old), "{old:#06x}");
        }
        // Broadwell (0x16xx), Skylake (0x19xx), Tiger Lake (0x9axx), Arc (0x56xx).
        for new in [0x1616, 0x1916, 0x9a49, 0x56a0, 0x7d55] {
            assert!(!intel_gen_old(new), "{new:#06x}");
        }
    }
}
