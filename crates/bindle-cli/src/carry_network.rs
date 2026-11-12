//! Carry the live ISO's Wi-Fi into the installed system.
//!
//! On the ISO you connect with `iwctl`, which saves each network in
//! `/var/lib/iwd/<name>.psk` (or `.open`). The installed system uses
//! NetworkManager (Bindle's preset), which never reads those, so without
//! this the first boot is offline. Each iwd network becomes a
//! NetworkManager keyfile in `<root>/etc/NetworkManager/system-connections`
//! (root-only, 0600), so the network asked for once on the ISO is the one the
//! new system joins. Enterprise (`.8021x`) networks are skipped: they need
//! certificates and identities iwd stores differently.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::apply::sys::{create_dirs, write_atomic};

pub const IWD_DIR: &str = "/var/lib/iwd";
const NM_DIR: &str = "etc/NetworkManager/system-connections";

#[derive(Debug, PartialEq, Eq)]
pub struct Network {
    pub ssid: Vec<u8>,
    /// None for an open network.
    pub psk: Option<String>,
    pub hidden: bool,
}

/// The SSID from an iwd file stem: the name itself, or `=` + hex when the
/// SSID has characters iwd won't put in a file name.
fn ssid_from_stem(stem: &str) -> Option<Vec<u8>> {
    match stem.strip_prefix('=') {
        Some(hex) => {
            if hex.is_empty() || hex.len() % 2 != 0 {
                return None;
            }
            (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect()
        }
        None if stem.is_empty() => None,
        None => Some(stem.as_bytes().to_vec()),
    }
}

/// One iwd network file. `kind` is the extension: `psk` or `open`.
pub fn parse_iwd(stem: &str, kind: &str, text: &str) -> Option<Network> {
    let ssid = ssid_from_stem(stem)?;
    if ssid.len() > 32 {
        return None;
    }
    let (mut section, mut passphrase, mut key, mut hidden) = (String::new(), None, None, false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match (section.as_str(), k.trim()) {
            ("Security", "Passphrase") => passphrase = Some(v.to_string()),
            ("Security", "PreSharedKey") => key = Some(v.trim().to_string()),
            ("Settings", "Hidden") => hidden = v.trim() == "true",
            _ => {}
        }
    }
    let psk = match kind {
        "open" => None,
        // NetworkManager takes the passphrase, or the 64-hex-digit key iwd
        // derives from it when the passphrase wasn't kept.
        "psk" => Some(
            passphrase
                .filter(|p| (8..=63).contains(&p.len()))
                .or_else(|| key.filter(|k| k.len() == 64 && k.chars().all(|c| c.is_ascii_hexdigit())))?,
        ),
        _ => return None,
    };
    Some(Network { ssid, psk, hidden })
}

/// An SSID as a keyfile value: plain text when that round-trips, else the
/// `b1;b2;…;` byte list NetworkManager also accepts.
fn keyfile_ssid(ssid: &[u8]) -> String {
    let plain = std::str::from_utf8(ssid).ok().filter(|s| {
        !s.is_empty()
            && s.chars().all(|c| !c.is_control() && c != ';' && c != '\\')
            && s.trim() == *s
            && !s.chars().all(|c| c.is_ascii_digit())
    });
    match plain {
        Some(s) => s.to_string(),
        None => ssid.iter().map(|b| format!("{b};")).collect(),
    }
}

/// A keyfile value: escape the characters the GKeyFile format reserves.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let last = s.chars().count().saturating_sub(1);
    for (i, c) in s.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            ' ' if i == 0 || i == last => out.push_str("\\s"),
            c => out.push(c),
        }
    }
    out
}

/// A file name for the keyfile: the readable SSID, or its hex.
fn file_name(ssid: &[u8]) -> String {
    let readable = std::str::from_utf8(ssid).ok().filter(|s| {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_. ".contains(c)) && !s.starts_with('.')
    });
    let stem = match readable {
        Some(s) => s.replace(' ', "_"),
        None => ssid.iter().map(|b| format!("{b:02x}")).collect(),
    };
    format!("bindle-{stem}.nmconnection")
}

pub fn keyfile(net: &Network, uuid: &str) -> String {
    let id = escape(&String::from_utf8_lossy(&net.ssid));
    let mut s = format!(
        "# Managed by Bindle: the Wi-Fi you joined on the install ISO\n\
         [connection]\nid={id}\nuuid={uuid}\ntype=wifi\nautoconnect=true\n\n\
         [wifi]\nmode=infrastructure\nssid={}\n",
        keyfile_ssid(&net.ssid)
    );
    if net.hidden {
        s.push_str("hidden=true\n");
    }
    if let Some(psk) = &net.psk {
        s.push_str(&format!("\n[wifi-security]\nkey-mgmt=wpa-psk\npsk={}\n", escape(psk)));
    }
    s.push_str("\n[ipv4]\nmethod=auto\n\n[ipv6]\naddr-gen-mode=stable-privacy\nmethod=auto\n");
    s
}

fn new_uuid() -> String {
    fs::read_to_string("/proc/sys/kernel/random/uuid").map(|u| u.trim().to_string()).unwrap_or_else(|_| {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let h = format!("{n:032x}");
        format!("{}-{}-4{}-8{}-{}", &h[0..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
    })
}

/// Every network iwd saved in `from`, sorted by file name.
pub fn read_iwd(from: &Path) -> Result<Vec<Network>> {
    let mut entries: Vec<_> = match fs::read_dir(from) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("read {}", from.display())),
    };
    entries.sort();
    let mut out = Vec::new();
    for path in entries {
        let (Some(stem), Some(kind)) =
            (path.file_stem().and_then(|s| s.to_str()), path.extension().and_then(|s| s.to_str()))
        else {
            continue;
        };
        if kind != "psk" && kind != "open" {
            continue;
        }
        let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        match parse_iwd(stem, kind, &text) {
            Some(n) => out.push(n),
            None => eprintln!("[bindle] skipped {} (no usable passphrase or name)", path.display()),
        }
    }
    Ok(out)
}

/// Write a keyfile per network into `root`; returns the SSIDs written.
pub fn carry(from: &Path, root: &Path) -> Result<Vec<String>> {
    let nets = read_iwd(from)?;
    if nets.is_empty() {
        return Ok(Vec::new());
    }
    let dir = root.join(NM_DIR);
    create_dirs(&dir)?;
    let mut done = Vec::new();
    for net in &nets {
        let path = dir.join(file_name(&net.ssid));
        write_atomic(&path, keyfile(net, &new_uuid()).as_bytes(), 0o600)?;
        done.push(String::from_utf8_lossy(&net.ssid).into_owned());
    }
    Ok(done)
}

pub fn run(from: &Path, root: &Path) -> Result<()> {
    let done = carry(from, root)?;
    if done.is_empty() {
        println!("No Wi-Fi networks saved on this ISO; nothing to carry over.");
    } else {
        println!("Saved Wi-Fi for the new system: {}", done.join(", "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passphrase_networks_carry_over() {
        let n = parse_iwd("HomeNet", "psk", "[Security]\nPassphrase=hunter2hunter2\nPreSharedKey=ab\n").unwrap();
        assert_eq!(n, Network { ssid: b"HomeNet".to_vec(), psk: Some("hunter2hunter2".into()), hidden: false });
        let k = keyfile(&n, "u-1");
        assert!(k.contains("ssid=HomeNet\n"));
        assert!(k.contains("key-mgmt=wpa-psk\npsk=hunter2hunter2\n"));
        assert!(k.contains("uuid=u-1\n"));
        assert!(!k.contains("hidden"));
    }

    #[test]
    fn derived_key_when_no_passphrase() {
        let key = "a".repeat(64);
        let n = parse_iwd("Cafe", "psk", &format!("[Security]\nPreSharedKey={key}\n")).unwrap();
        assert_eq!(n.psk.as_deref(), Some(key.as_str()));
        assert!(parse_iwd("Cafe", "psk", "[Security]\nPreSharedKey=short\n").is_none());
        assert!(parse_iwd("Cafe", "psk", "").is_none());
    }

    #[test]
    fn open_and_hidden_networks() {
        let n = parse_iwd("Library", "open", "[Settings]\nHidden=true\n").unwrap();
        assert_eq!(n.psk, None);
        let k = keyfile(&n, "u");
        assert!(k.contains("hidden=true\n"));
        assert!(!k.contains("wifi-security"));
        assert!(parse_iwd("Work", "8021x", "").is_none());
    }

    #[test]
    fn hex_named_ssids_become_byte_lists() {
        // "a;b" can't be a plain keyfile value.
        let n = parse_iwd("=613b62", "open", "").unwrap();
        assert_eq!(n.ssid, b"a;b");
        assert!(keyfile(&n, "u").contains("ssid=97;59;98;\n"));
        assert_eq!(file_name(&n.ssid), "bindle-613b62.nmconnection");
        assert!(parse_iwd("=6", "open", "").is_none());
        assert!(parse_iwd("=zz", "open", "").is_none());
        // An all-digit SSID would read as a byte list, so it's written as one.
        assert_eq!(keyfile_ssid(b"1234"), "49;50;51;52;");
    }

    #[test]
    fn names_and_values_are_safe() {
        assert_eq!(file_name(b"My Home"), "bindle-My_Home.nmconnection");
        assert_eq!(file_name(b"../x"), format!("bindle-{}.nmconnection", "2e2e2f78"));
        assert_eq!(escape(" pass\\word "), "\\spass\\\\word\\s");
    }

    #[test]
    fn carry_writes_private_keyfiles() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = std::env::temp_dir().join(format!("bindle-carry-{}", std::process::id()));
        let (iwd, root) = (tmp.join("iwd"), tmp.join("root"));
        fs::create_dir_all(&iwd).unwrap();
        fs::write(iwd.join("Home.psk"), "[Security]\nPassphrase=correcthorse\n").unwrap();
        fs::write(iwd.join("Work.8021x"), "[Security]\nEAP-Method=PEAP\n").unwrap();
        assert_eq!(carry(&iwd, &root).unwrap(), ["Home"]);
        let f = root.join(NM_DIR).join("bindle-Home.nmconnection");
        assert!(fs::read_to_string(&f).unwrap().contains("psk=correcthorse"));
        assert_eq!(fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(carry(&tmp.join("missing"), &root).unwrap().is_empty());
        fs::remove_dir_all(&tmp).unwrap();
    }
}
