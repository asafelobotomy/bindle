//! What `bindle install` asks the live ISO: Wi-Fi (through iwd's `iwctl`,
//! which the ISO ships), the disks it can install to, and the firmware type.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct WifiNetwork {
    pub ssid: String,
    /// iwd's security type: `psk`, `open` or `8021x`.
    pub security: String,
    /// 0–4 bars.
    pub signal: u8,
    pub connected: bool,
}

/// The first wireless interface (iwd names its station after it).
pub fn wifi_device() -> Option<String> {
    let mut names: Vec<String> = fs::read_dir("/sys/class/net")
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("wireless").is_dir() || e.path().join("phy80211").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names.into_iter().next()
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// `iwctl station <dev> get-networks`: a table after a `---` rule, each row
/// `[>] <ssid…> <security> <signal stars>`, the connected one marked `>`.
pub fn parse_networks(output: &str) -> Vec<WifiNetwork> {
    let text = strip_ansi(output);
    let mut rows = text.lines().skip_while(|l| !l.trim_start().starts_with("---")).skip(1);
    let mut out = Vec::new();
    for line in rows.by_ref() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("---") {
            continue;
        }
        let (connected, rest) = match line.strip_prefix('>') {
            Some(r) => (true, r.trim_start()),
            None => (false, line),
        };
        let mut words: Vec<&str> = rest.split_whitespace().collect();
        let Some(stars) = words.pop().filter(|w| w.chars().all(|c| c == '*')) else { continue };
        let Some(security) = words.pop().filter(|w| ["psk", "open", "8021x", "wep"].contains(w)) else { continue };
        // Keep the SSID's own spacing: cut the row before the security word.
        let ssid = match rest.rfind(security) {
            Some(i) => rest[..i].trim_end().to_string(),
            None => words.join(" "),
        };
        if ssid.is_empty() {
            continue;
        }
        out.push(WifiNetwork { ssid, security: security.into(), signal: stars.len().min(4) as u8, connected });
    }
    out
}

/// Scan and list nearby networks, strongest first (iwctl's order).
pub fn scan(dev: &str) -> Result<Vec<WifiNetwork>> {
    let _ = Command::new("iwctl").args(["station", dev, "scan"]).output();
    std::thread::sleep(std::time::Duration::from_secs(3));
    let out = Command::new("iwctl").args(["station", dev, "get-networks"]).output().context("run iwctl")?;
    Ok(parse_networks(&String::from_utf8_lossy(&out.stdout)))
}

/// Join `ssid`. iwd saves it in /var/lib/iwd, which the install carries over
/// to the new system (`bindle carry-network`).
pub fn connect(dev: &str, ssid: &str, passphrase: Option<&str>) -> Result<()> {
    let mut c = Command::new("iwctl");
    if let Some(p) = passphrase {
        c.args(["--passphrase", p]);
    }
    let out = c.args(["station", dev, "connect", ssid]).output().context("run iwctl")?;
    if !out.status.success() {
        let msg = strip_ansi(&String::from_utf8_lossy(&out.stdout)) + &String::from_utf8_lossy(&out.stderr);
        bail!("couldn't join {ssid}: {}", msg.trim());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    pub path: String,
    pub bytes: u64,
    pub model: String,
    pub transport: String,
}

impl Disk {
    pub fn label(&self) -> String {
        let model = if self.model.is_empty() { String::new() } else { format!(" · {}", self.model) };
        let tran = if self.transport.is_empty() { String::new() } else { format!(" · {}", self.transport) };
        format!("{} · {:.0} GiB{model}{tran}", self.path, self.bytes as f64 / (1u64 << 30) as f64)
    }
}

/// Whole disks from `lsblk -J -b -d`, without read-only, removable optical,
/// loop and zram devices, and without `exclude` (the ISO's own boot disk).
pub fn parse_disks(json: &str, exclude: Option<&str>) -> Vec<Disk> {
    let v: Value = serde_json::from_str(json).unwrap_or(Value::Null);
    let s = |d: &Value, k: &str| d.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let flag = |d: &Value, k: &str| d.get(k).is_some_and(|x| x.as_bool() == Some(true) || x.as_str() == Some("1"));
    v.get("blockdevices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|d| s(d, "type") == "disk" && !flag(d, "ro"))
        .filter(|d| !s(d, "name").starts_with("zram") && !s(d, "name").starts_with("loop"))
        .map(|d| Disk {
            path: s(d, "path"),
            bytes: d.get("size").and_then(|x| x.as_u64().or_else(|| x.as_str()?.parse().ok())).unwrap_or(0),
            model: s(d, "model"),
            transport: s(d, "tran"),
        })
        .filter(|d| !d.path.is_empty() && Some(d.path.as_str()) != exclude)
        .collect()
}

/// The disk the ISO booted from (holding /run/archiso/bootmnt), if any.
fn boot_disk() -> Option<String> {
    let out = Command::new("findmnt").args(["-n", "-o", "SOURCE", "/run/archiso/bootmnt"]).output().ok()?;
    let part = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if part.is_empty() {
        return None;
    }
    let out = Command::new("lsblk").args(["-n", "-d", "-o", "PKNAME", &part]).output().ok()?;
    let parent = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Some(if parent.is_empty() { part } else { format!("/dev/{parent}") })
}

pub fn disks() -> Vec<Disk> {
    let out = Command::new("lsblk").args(["-J", "-b", "-d", "-o", "NAME,PATH,SIZE,MODEL,TYPE,RO,TRAN"]).output();
    match out {
        Ok(o) => parse_disks(&String::from_utf8_lossy(&o.stdout), boot_disk().as_deref()),
        Err(_) => Vec::new(),
    }
}

pub fn uefi() -> bool {
    Path::new("/sys/firmware/efi").is_dir()
}

/// A timezone the new system has: `Area/City` under /usr/share/zoneinfo.
pub fn valid_timezone(tz: &str) -> bool {
    !tz.is_empty() && !tz.contains("..") && !tz.starts_with('/') && Path::new("/usr/share/zoneinfo").join(tz).is_file()
}

/// The ISO's own timezone when it isn't plain UTC, else UTC.
pub fn default_timezone() -> String {
    fs::read_link("/etc/localtime")
        .ok()
        .and_then(|p| p.to_string_lossy().split("zoneinfo/").nth(1).map(String::from))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "UTC".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETWORKS: &str = "                               Available networks\n\
\u{1b}[0m--------------------------------------------------------------------------------\n\
      Network name                      Security            Signal\n\
--------------------------------------------------------------------------------\n\
  \u{1b}[1;90m> \u{1b}[0mHome Net                          psk                 ****\n\
    Cafe                              open                ***\u{1b}[1;90m*\u{1b}[0m\n\
    Office                            8021x               **\n\
    weird row\n";

    #[test]
    fn parses_iwctl_networks() {
        let n = parse_networks(NETWORKS);
        assert_eq!(n.len(), 3, "{n:?}");
        assert_eq!(n[0], WifiNetwork { ssid: "Home Net".into(), security: "psk".into(), signal: 4, connected: true });
        assert_eq!((n[1].ssid.as_str(), n[1].security.as_str(), n[1].connected), ("Cafe", "open", false));
        assert_eq!(n[2].signal, 2);
        assert!(parse_networks("No networks available\n").is_empty());
    }

    #[test]
    fn lists_installable_disks() {
        let json = r#"{"blockdevices":[
            {"name":"nvme0n1","path":"/dev/nvme0n1","size":512110190592,"model":"Samsung SSD","type":"disk","ro":false,"tran":"nvme"},
            {"name":"sda","path":"/dev/sda","size":"31914983424","model":"USB Stick ","type":"disk","ro":false,"tran":"usb"},
            {"name":"sr0","path":"/dev/sr0","size":1073741312,"model":"DVD","type":"rom","ro":false,"tran":"sata"},
            {"name":"zram0","path":"/dev/zram0","size":4294967296,"model":null,"type":"disk","ro":false,"tran":null},
            {"name":"loop0","path":"/dev/loop0","size":800000000,"model":null,"type":"loop","ro":true,"tran":null}
        ]}"#;
        let d = parse_disks(json, Some("/dev/sda"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].path, "/dev/nvme0n1");
        assert!(d[0].label().starts_with("/dev/nvme0n1 · 477 GiB · Samsung SSD · nvme"));
        assert_eq!(parse_disks(json, None)[1].model, "USB Stick");
        assert!(parse_disks("not json", None).is_empty());
    }

    #[test]
    fn timezones_stay_inside_zoneinfo() {
        assert!(!valid_timezone("../../etc/passwd"));
        assert!(!valid_timezone("/etc/passwd"));
        assert!(!valid_timezone(""));
    }
}
