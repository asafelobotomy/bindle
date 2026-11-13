//! The system's state as data, from command output and /sys: free space,
//! power, kernels and DKMS modules, failed services, packages a library
//! update broke, uQ's Hyprland pin and setup report. Pure; every parser is
//! tested on real output.

use std::cmp::Ordering;

use serde::Serialize;

/// A mounted filesystem and its free space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mount {
    pub target: String,
    pub fstype: String,
    pub avail_mb: u64,
}

/// `df --output=target,fstype,avail -BM <paths>`: one line per path (the
/// mount it is on), after a header. The same mount twice is listed once.
pub fn df(text: &str) -> Vec<Mount> {
    let mut out: Vec<Mount> = vec![];
    for l in text.lines().skip(1) {
        let w: Vec<&str> = l.split_whitespace().collect();
        let [target, fstype, avail] = w[..] else { continue };
        let Ok(avail_mb) = avail.trim_end_matches('M').parse() else { continue };
        if !out.iter().any(|m| m.target == target) {
            out.push(Mount { target: target.into(), fstype: fstype.into(), avail_mb });
        }
    }
    out
}

/// The EFI system partition: a FAT filesystem mounted at /efi, /boot/efi or /boot.
pub fn efi(mounts: &[Mount]) -> Option<&Mount> {
    ["/efi", "/boot/efi", "/boot"]
        .iter()
        .find_map(|t| mounts.iter().find(|m| m.target == *t && m.fstype.starts_with("vfat")))
}

/// One line of `dkms status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dkms {
    pub module: String,
    /// Empty for a module added but not built for any kernel.
    pub kernel: String,
    pub state: String,
}

/// `nvidia/580.196.07, 7.3.4-zen2-1-zen, x86_64: installed` (and the older
/// `nvidia, 580, 6.1.1-arch1-1, x86_64: installed`, and `nvidia/580: added`).
pub fn dkms(text: &str) -> Vec<Dkms> {
    text.lines()
        .filter_map(|l| {
            let (left, state) = l.rsplit_once(": ")?;
            let parts: Vec<&str> = left.split(", ").collect();
            let module = parts.first()?.split('/').next()?.trim().to_string();
            let kernel = if parts.len() >= 3 { parts[parts.len() - 2].trim().to_string() } else { String::new() };
            let state = state.split_whitespace().next().unwrap_or_default().to_string();
            Some(Dkms { module, kernel, state })
        })
        .collect()
}

/// An installed kernel: a directory in /usr/lib/modules with a `pkgbase`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Kernel {
    pub version: String,
    pub pkgbase: String,
}

/// DKMS modules with no `installed` build for one of the kernels, as
/// (module, kernel version).
pub fn dkms_missing(kernels: &[Kernel], entries: &[Dkms]) -> Vec<(String, String)> {
    let mut modules: Vec<&str> = entries.iter().map(|d| d.module.as_str()).collect();
    modules.dedup();
    let mut out = vec![];
    for k in kernels {
        for m in &modules {
            let built = entries.iter().any(|d| d.module == *m && d.kernel == k.version && d.state == "installed");
            if !built {
                out.push((m.to_string(), k.version.clone()));
            }
        }
    }
    out
}

/// Boot images mkinitcpio should have made for each kernel, missing from a
/// listing of /boot. With no `vmlinuz-*` there at all the system boots some
/// other way (unified kernel images), and nothing is reported.
pub fn images_missing(kernels: &[Kernel], boot: &[String]) -> Vec<String> {
    if !boot.iter().any(|f| f.starts_with("vmlinuz-")) {
        return vec![];
    }
    kernels
        .iter()
        .flat_map(|k| [format!("vmlinuz-{}", k.pkgbase), format!("initramfs-{}.img", k.pkgbase)])
        .filter(|f| !boot.contains(f))
        .collect()
}

/// Unit names from `systemctl --failed --plain --no-legend`.
pub fn failed_units(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.trim_start_matches(['●', '*', ' ']).split_whitespace().next())
        .filter(|u| u.contains('.'))
        .map(String::from)
        .collect()
}

/// A package `checkrebuild` (rebuild-detector) says needs a rebuild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rebuild {
    /// `foreign` (AUR, built here) or the repo it came from.
    pub repo: String,
    pub name: String,
}

/// `foreign\tpython-foo` lines.
pub fn checkrebuild(text: &str) -> Vec<Rebuild> {
    text.lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let (repo, name) = (w.next()?, w.next()?);
            Some(Rebuild { repo: repo.into(), name: name.into() })
        })
        .collect()
}

/// Power: on battery, and how full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Power {
    pub on_battery: bool,
    pub capacity: Option<u8>,
}

/// From /sys/class/power_supply: each supply's `type`, `online` and
/// `capacity`. No battery: None (a desktop). A battery with no mains
/// supply online: on battery.
pub fn power(supplies: &[(String, Option<bool>, Option<u8>)]) -> Option<Power> {
    let battery = supplies.iter().find(|(t, _, _)| t == "Battery")?;
    let mains = supplies.iter().any(|(t, online, _)| t == "Mains" && *online == Some(true));
    Some(Power { on_battery: !mains, capacity: battery.2 })
}

/// The Hyprland versions uq-hyprland-pin allows, from its `Depends On`
/// (`hyprland>=0.56.2  hyprland<0.58.0`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Pin {
    pub at_least: Option<String>,
    pub below: Option<String>,
}

pub fn pin(depends: &str) -> Pin {
    let mut p = Pin::default();
    for d in depends.split_whitespace() {
        if let Some(v) = d.strip_prefix("hyprland>=") {
            p.at_least = Some(v.into());
        } else if let Some(v) = d.strip_prefix("hyprland<") {
            p.below = Some(v.into());
        }
    }
    p
}

impl Pin {
    /// `version` (a package version, `0.58.0-1`) is in the pinned range.
    pub fn allows(&self, version: &str) -> bool {
        let v = version.split_once('-').map_or(version, |(v, _)| v);
        self.at_least.as_ref().is_none_or(|lo| vercmp(v, lo) != Ordering::Less)
            && self.below.as_ref().is_none_or(|hi| vercmp(v, hi) == Ordering::Less)
    }
}

/// What `uqctl setup --apps --json` did, one line per file.
pub fn uq_setup(json: &str) -> Result<Vec<String>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("unexpected uqctl output: {e}"))?;
    if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
        return Err(msg.to_string());
    }
    let line = |f: &serde_json::Value| {
        let path = f["path"].as_str().unwrap_or("?");
        match f["outcome"].as_str() {
            Some("installed") => format!("wrote {path}"),
            _ => format!("kept {path} (yours; uQ's lines aren't in it)"),
        }
    };
    let mut out = vec![line(&v["hyprland_lua"])];
    out.extend(v["apps"].as_array().into_iter().flatten().map(line));
    Ok(out)
}

/// Data `age` days old (None: never fetched) needs a refresh after `max` days.
pub fn stale(age: Option<i64>, max: i64) -> bool {
    age.is_none_or(|a| a >= max)
}

/// pacman's version order, enough for package versions: an optional
/// `epoch:`, then runs of digits (numerically) and letters (alphabetically),
/// separated by anything else; a letter run sorts before a digit run, and
/// a version with more segments is newer (`1.0.1 > 1.0`, `1.0rc1 < 1.0`).
pub fn vercmp(a: &str, b: &str) -> Ordering {
    let epoch = |s: &str| match s.split_once(':') {
        Some((e, rest)) if e.chars().all(|c| c.is_ascii_digit()) => (e.parse::<u64>().unwrap_or(0), rest.to_string()),
        _ => (0, s.to_string()),
    };
    let ((ea, ra), (eb, rb)) = (epoch(a), epoch(b));
    ea.cmp(&eb).then_with(|| segments(&ra, &rb))
}

fn split(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    for c in s.chars() {
        let same = cur.chars().next().is_some_and(|f| f.is_ascii_digit() == c.is_ascii_digit());
        if !c.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else if cur.is_empty() || same {
            cur.push(c);
        } else {
            out.push(std::mem::replace(&mut cur, c.to_string()));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn segments(a: &str, b: &str) -> Ordering {
    let (sa, sb) = (split(a), split(b));
    for (x, y) in sa.iter().zip(&sb) {
        let (xd, yd) = (x.starts_with(|c: char| c.is_ascii_digit()), y.starts_with(|c: char| c.is_ascii_digit()));
        let o = match (xd, yd) {
            (true, true) => {
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                x.len().cmp(&y.len()).then_with(|| x.cmp(y))
            }
            (false, false) => x.cmp(y),
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    // `1.0rc1` < `1.0`: a trailing letter run is older; more numbers, newer.
    match sa.len().cmp(&sb.len()) {
        Ordering::Greater if sa[sb.len()].starts_with(|c: char| c.is_ascii_alphabetic()) => Ordering::Less,
        Ordering::Less if sb[sa.len()].starts_with(|c: char| c.is_ascii_alphabetic()) => Ordering::Greater,
        o => o,
    }
}

#[cfg(test)]
#[path = "system_tests.rs"]
mod tests;
