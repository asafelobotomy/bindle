//! Which disk the live ISO was started from, so `bindle install` never
//! offers it as the install target. The kernel command line says where
//! archiso found its files (`archisosearchuuid=`, `archisolabel=`,
//! `archisodevice=`, or `img_dev=` for an ISO file on another disk); that
//! still holds after `copytoram`, when nothing of the medium is mounted.
//! `lsblk`'s tree then gives the whole disk each one sits on. Pure: the
//! callers read /proc/cmdline and run lsblk.

use serde_json::Value;

/// One way the command line names the boot medium.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec {
    Uuid(String),
    PartUuid(String),
    Label(String),
    /// A device path (`/dev/sdb1`, `/dev/mapper/ventoy`, `/dev/dm-0`).
    Path(String),
}

/// What is known about the boot medium.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Medium {
    /// Not started from archiso (a dry run on an installed system).
    NotLive,
    /// The whole disks it is on: never offered for the install.
    Disks(Vec<String>),
    /// archiso named a medium that couldn't be matched to a disk.
    Unknown,
}

impl Medium {
    pub fn contains(&self, disk: &str) -> bool {
        matches!(self, Medium::Disks(d) if d.iter().any(|x| x == disk))
    }
}

/// `UUID=…`, `/dev/disk/by-label/…` and the like, or a plain device path.
pub fn spec(value: &str) -> Option<Spec> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    let pairs = [
        ("UUID=", 0),
        ("PARTUUID=", 1),
        ("LABEL=", 2),
        ("/dev/disk/by-uuid/", 0),
        ("/dev/disk/by-partuuid/", 1),
        ("/dev/disk/by-label/", 2),
    ];
    for (prefix, kind) in pairs {
        if let Some(rest) = v.strip_prefix(prefix) {
            let rest = unescape(rest);
            return Some(match kind {
                0 => Spec::Uuid(rest),
                1 => Spec::PartUuid(rest),
                _ => Spec::Label(rest),
            });
        }
    }
    v.starts_with("/dev/").then(|| Spec::Path(v.to_string()))
}

/// udev writes a space in a label as `\x20`.
fn unescape(s: &str) -> String {
    s.replace("\\x20", " ")
}

/// Every place the kernel command line says the ISO came from.
pub fn boot_specs(cmdline: &str) -> Vec<Spec> {
    let mut out = Vec::new();
    for word in cmdline.split_whitespace() {
        let Some((key, value)) = word.split_once('=') else { continue };
        let s = match key {
            "archisosearchuuid" => Some(Spec::Uuid(value.to_string())),
            "archisolabel" => Some(Spec::Label(unescape(value))),
            "archisodevice" | "img_dev" => spec(value),
            _ => None,
        };
        if let Some(s) = s.filter(|s| !out.contains(s)) {
            out.push(s);
        }
    }
    out
}

/// True when the kernel command line is archiso's (any `archiso*=` word).
pub fn archiso_cmdline(cmdline: &str) -> bool {
    cmdline.split_whitespace().any(|w| w.starts_with("archiso") && w.contains('='))
}

fn text<'a>(node: &'a Value, key: &str) -> &'a str {
    node.get(key).and_then(Value::as_str).unwrap_or("")
}

fn matches(node: &Value, spec: &Spec) -> bool {
    let eq = |key: &str, want: &str| !want.is_empty() && text(node, key) == want;
    match spec {
        Spec::Uuid(u) => !u.is_empty() && text(node, "uuid").eq_ignore_ascii_case(u),
        Spec::PartUuid(u) => !u.is_empty() && text(node, "partuuid").eq_ignore_ascii_case(u),
        Spec::Label(l) => eq("label", l),
        Spec::Path(p) => {
            eq("path", p)
                || [text(node, "kname"), text(node, "name")].iter().any(|n| !n.is_empty() && format!("/dev/{n}") == *p)
        }
    }
}

fn subtree_matches(node: &Value, specs: &[Spec]) -> bool {
    specs.iter().any(|s| matches(node, s))
        || node.get("children").and_then(Value::as_array).into_iter().flatten().any(|c| subtree_matches(c, specs))
}

/// The whole disks (`lsblk -J` tree, not `-d`) holding any of `specs`. A
/// device-mapper source such as Ventoy's sits under the partition it maps,
/// so it leads to that partition's disk.
pub fn medium_disks(specs: &[Spec], lsblk_tree: &str) -> Vec<String> {
    let v: Value = serde_json::from_str(lsblk_tree).unwrap_or(Value::Null);
    let mut out: Vec<String> = Vec::new();
    for top in v.get("blockdevices").and_then(Value::as_array).into_iter().flatten() {
        let path = text(top, "path").to_string();
        if text(top, "type") == "disk" && !path.is_empty() && subtree_matches(top, specs) && !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// What is known, from the command line's specs and lsblk's tree.
pub fn medium(archiso: bool, specs: &[Spec], lsblk_tree: &str) -> Medium {
    if !archiso && specs.is_empty() {
        return Medium::NotLive;
    }
    match medium_disks(specs, lsblk_tree) {
        d if d.is_empty() => Medium::Unknown,
        d => Medium::Disks(d),
    }
}

/// The open LUKS mappings (`crypt`) on a disk, from `lsblk -J <disk>`:
/// what an install that stopped partway may have left open.
pub fn crypt_mappings(lsblk_tree: &str) -> Vec<String> {
    fn walk(node: &Value, out: &mut Vec<String>) {
        if text(node, "type") == "crypt" && !out.iter().any(|n| n == text(node, "name")) {
            out.push(text(node, "name").to_string());
        }
        for c in node.get("children").and_then(Value::as_array).into_iter().flatten() {
            walk(c, out);
        }
    }
    let v: Value = serde_json::from_str(lsblk_tree).unwrap_or(Value::Null);
    let mut out = Vec::new();
    for top in v.get("blockdevices").and_then(Value::as_array).into_iter().flatten() {
        walk(top, &mut out);
    }
    out
}

#[cfg(test)]
#[path = "medium_tests.rs"]
mod tests;
