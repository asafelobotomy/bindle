//! What `bindle install` asks the live ISO: Wi-Fi (through iwd's `iwctl`,
//! which the ISO ships), the disks it can install to (never the one it
//! booted from), the firmware type, keymaps and locales.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

mod medium;
pub use medium::Medium;
use medium::Spec;

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

pub(crate) fn strip_ansi(s: &str) -> String {
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Disk {
    pub path: String,
    pub bytes: u64,
    pub model: String,
    pub transport: String,
    /// Removable or on USB: not where a system is usually installed.
    pub removable: bool,
    /// The boot medium couldn't be identified and this could be it.
    pub maybe_medium: bool,
}

impl Disk {
    /// `/dev/sda · 29 GiB · SanDisk · usb`: what the wipe question names.
    pub fn label(&self) -> String {
        let model = if self.model.is_empty() { String::new() } else { format!(" · {}", self.model) };
        let tran = if self.transport.is_empty() { String::new() } else { format!(" · {}", self.transport) };
        let note = if self.maybe_medium { " · may be the install USB" } else { "" };
        format!("{} · {:.0} GiB{model}{tran}{note}", self.path, self.bytes as f64 / (1u64 << 30) as f64)
    }

    /// Still the same disk: size and model as when it was chosen.
    pub fn same_as(&self, other: &Disk) -> bool {
        self.path == other.path && self.bytes == other.bytes && self.model == other.model
    }
}

/// Whole disks from `lsblk -J -b -d`, without read-only, removable optical,
/// loop and zram devices, and without the ISO's own boot medium. When that
/// medium couldn't be identified, removable disks are marked as maybe it.
pub fn parse_disks(json: &str, boot: &Medium) -> Vec<Disk> {
    let v: Value = serde_json::from_str(json).unwrap_or(Value::Null);
    let s = |d: &Value, k: &str| d.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let flag = |d: &Value, k: &str| d.get(k).is_some_and(|x| x.as_bool() == Some(true) || x.as_str() == Some("1"));
    v.get("blockdevices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|d| s(d, "type") == "disk" && !flag(d, "ro"))
        .filter(|d| !s(d, "name").starts_with("zram") && !s(d, "name").starts_with("loop"))
        .map(|d| {
            let removable = flag(d, "rm") || s(d, "tran") == "usb";
            Disk {
                path: s(d, "path"),
                bytes: d.get("size").and_then(|x| x.as_u64().or_else(|| x.as_str()?.parse().ok())).unwrap_or(0),
                model: s(d, "model"),
                transport: s(d, "tran"),
                removable,
                maybe_medium: removable && *boot == Medium::Unknown,
            }
        })
        .filter(|d| !d.path.is_empty() && !boot.contains(&d.path))
        .collect()
}

/// The row to start on: the largest fixed (not removable, not USB) disk,
/// else the largest that isn't maybe the boot medium.
pub fn preferred(disks: &[Disk]) -> usize {
    let best = |ok: &dyn Fn(&Disk) -> bool| {
        disks
            .iter()
            .enumerate()
            .filter(|(_, d)| ok(d))
            .max_by_key(|(i, d)| (d.bytes, std::cmp::Reverse(*i)))
            .map(|(i, _)| i)
    };
    best(&|d| !d.removable && !d.maybe_medium).or_else(|| best(&|d| !d.maybe_medium)).unwrap_or(0)
}

fn run_text(prog: &str, args: &[&str]) -> String {
    Command::new(prog).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// The disk(s) the ISO was started from: the kernel command line (works
/// after copytoram and for an ISO file on a disk), and what is mounted on
/// /run/archiso/bootmnt, each resolved to its whole disk.
pub fn boot_medium() -> Medium {
    let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let mut specs = medium::boot_specs(&cmdline);
    let bootmnt = run_text("findmnt", &["-n", "-o", "SOURCE", "/run/archiso/bootmnt"]);
    specs.extend(medium::spec(bootmnt.trim()));
    // Paths also by their kernel names (/dev/mapper/ventoy is /dev/dm-0).
    let real: Vec<Spec> = specs
        .iter()
        .filter_map(|s| match s {
            Spec::Path(p) => fs::canonicalize(p).ok().map(|c| Spec::Path(c.display().to_string())),
            _ => None,
        })
        .collect();
    specs.extend(real);
    let tree = run_text("lsblk", &["-J", "-o", "NAME,KNAME,PATH,UUID,PARTUUID,LABEL,TYPE"]);
    medium::medium(medium::archiso_cmdline(&cmdline), &specs, &tree)
}

const DISK_COLUMNS: &str = "NAME,PATH,SIZE,MODEL,TYPE,RO,RM,TRAN";

/// The disks to offer, and the boot medium left out of them.
pub fn disks() -> (Vec<Disk>, Medium) {
    let boot = boot_medium();
    (parse_disks(&run_text("lsblk", &["-J", "-b", "-d", "-o", DISK_COLUMNS]), &boot), boot)
}

/// One disk as it is now (re-read just before it is erased).
pub fn disk_now(path: &str) -> Option<Disk> {
    parse_disks(&run_text("lsblk", &["-J", "-b", "-d", "-o", DISK_COLUMNS, path]), &Medium::NotLive).into_iter().next()
}

/// After a failed install: unmount /mnt and close the LUKS mappings left
/// open on `disks`, so another try starts clean. What it did, in words.
pub fn release_target(disks: &[String]) -> Vec<String> {
    let mut done = Vec::new();
    let mounted = Command::new("mountpoint").args(["-q", "/mnt"]).status().is_ok_and(|s| s.success());
    if mounted && Command::new("umount").args(["-R", "/mnt"]).status().is_ok_and(|s| s.success()) {
        done.push("unmounted /mnt".to_string());
    }
    for disk in disks {
        for name in medium::crypt_mappings(&run_text("lsblk", &["-J", "-o", "NAME,TYPE", disk])) {
            if Command::new("cryptsetup").args(["close", &name]).status().is_ok_and(|s| s.success()) {
                done.push(format!("closed the encrypted volume {name}"));
            }
        }
    }
    done
}

/// Console keymaps the ISO has (`us`, `uk`, `de-latin1`…), from
/// /usr/share/kbd/keymaps. None: none found, so nothing is checked.
pub fn keymaps() -> Option<Vec<String>> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if let Some(name) = keymap_name(&e.file_name().to_string_lossy()) {
                out.push(name.to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(Path::new("/usr/share/kbd/keymaps"), &mut out);
    out.sort();
    out.dedup();
    (!out.is_empty()).then_some(out)
}

/// `de-latin1.map.gz` → `de-latin1`.
pub fn keymap_name(file: &str) -> Option<&str> {
    file.strip_suffix(".map.gz").or_else(|| file.strip_suffix(".map")).filter(|n| !n.is_empty())
}

/// glibc's list of locales (`en_GB.UTF-8 UTF-8` per line). None: not here.
pub fn supported_locales() -> Option<String> {
    fs::read_to_string("/usr/share/i18n/SUPPORTED").ok().filter(|s| !s.trim().is_empty())
}

/// Switch the console's keyboard (`loadkeys`), so passwords are typed as
/// they will be at the boot passphrase prompt.
pub fn load_keys(layout: &str) -> Result<()> {
    let out = Command::new("loadkeys").arg(layout).output().context("run loadkeys")?;
    if !out.status.success() {
        bail!("loadkeys {layout}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

/// Login names the ISO itself has (`/etc/passwd`).
pub fn system_users() -> String {
    fs::read_to_string("/etc/passwd").unwrap_or_default()
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
        let d = parse_disks(json, &Medium::Disks(vec!["/dev/sda".into()]));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].path, "/dev/nvme0n1");
        assert!(d[0].label().starts_with("/dev/nvme0n1 · 477 GiB · Samsung SSD · nvme"));
        let all = parse_disks(json, &Medium::NotLive);
        assert_eq!(all[1].model, "USB Stick");
        assert!(all[1].removable && !all[0].removable && !all[1].maybe_medium);
        // Medium not identified: the USB stick stays, marked.
        let unsure = parse_disks(json, &Medium::Unknown);
        assert!(unsure[1].maybe_medium && unsure[1].label().ends_with("may be the install USB"));
        assert!(parse_disks("not json", &Medium::NotLive).is_empty());
    }

    #[test]
    fn starts_on_the_largest_fixed_disk() {
        let d = |path: &str, gib: u64, removable: bool| Disk {
            path: path.into(),
            bytes: gib << 30,
            removable,
            ..Disk::default()
        };
        // A big USB disk first, then two internal ones.
        let disks = [d("/dev/sda", 2000, true), d("/dev/sdb", 250, false), d("/dev/nvme0n1", 1000, false)];
        assert_eq!(preferred(&disks), 2);
        assert_eq!(preferred(&disks[..1]), 0);
        let unsure = [Disk { maybe_medium: true, ..d("/dev/sda", 64, true) }, d("/dev/sdb", 32, true)];
        assert_eq!(preferred(&unsure), 1);
        assert_eq!(preferred(&[]), 0);
        assert_eq!(keymap_name("de-latin1.map.gz"), Some("de-latin1"));
        assert_eq!(keymap_name("README"), None);
        let a = d("/dev/sdb", 250, false);
        assert!(a.same_as(&a.clone()) && !a.same_as(&Disk { model: "Other".into(), ..a.clone() }));
    }

    #[test]
    fn timezones_stay_inside_zoneinfo() {
        assert!(!valid_timezone("../../etc/passwd"));
        assert!(!valid_timezone("/etc/passwd"));
        assert!(!valid_timezone(""));
    }
}
