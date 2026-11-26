//! The facts `compat` judges: installed foreign packages from `pacman -Qmi`
//! and the AUR's new versions from its info API (`/rpc/v5/info`).

use std::collections::BTreeMap;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::compat::{Facts, Pkg};
use crate::parse::Update;
use crate::vercmp::Dep;

/// The AUR's info API; names are added as `arg[]=`.
pub const AUR_INFO: &str = "https://aur.archlinux.org/rpc/v5/info";

fn deps(list: &[&str]) -> Vec<Dep> {
    list.iter().filter_map(|d| Dep::parse(d)).collect()
}

/// `pacman -Qi` blocks (`Name : x`, long lists wrapped onto indented lines,
/// `None` for an empty list), by name.
pub fn installed(text: &str) -> BTreeMap<String, Pkg> {
    let mut out = BTreeMap::new();
    for block in text.split("\n\n") {
        let mut fields: BTreeMap<&str, String> = BTreeMap::new();
        let mut last = "";
        for line in block.lines() {
            match line.split_once(" : ") {
                Some((k, v)) if !line.starts_with(' ') => {
                    last = k.trim();
                    fields.insert(last, v.trim().to_string());
                }
                _ if !last.is_empty() && line.starts_with(' ') => {
                    if let Some(v) = fields.get_mut(last) {
                        v.push_str("  ");
                        v.push_str(line.trim());
                    }
                }
                _ => {}
            }
        }
        let list = |k: &str| -> Vec<Dep> {
            match fields.get(k).map(String::as_str) {
                None | Some("None") => vec![],
                Some(v) => deps(&v.split_whitespace().collect::<Vec<_>>()),
            }
        };
        if let (Some(name), Some(version)) = (fields.get("Name"), fields.get("Version")) {
            let pkg = Pkg {
                version: version.clone(),
                depends: list("Depends On"),
                conflicts: list("Conflicts With"),
                provides: list("Provides"),
            };
            out.insert(name.clone(), pkg);
        }
    }
    out
}

/// The AUR info API's answer, by name.
pub fn candidates(json: &str) -> Result<BTreeMap<String, Pkg>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("the AUR's answer isn't JSON ({e})"))?;
    if v.get("type").and_then(Value::as_str) == Some("error") {
        let why = v.get("error").and_then(Value::as_str).unwrap_or("an error");
        return Err(format!("the AUR said: {why}"));
    }
    let list = |r: &Value, k: &str| -> Vec<Dep> {
        let items: Vec<&str> =
            r.get(k).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
        deps(&items)
    };
    let mut out = BTreeMap::new();
    for r in v.get("results").and_then(Value::as_array).into_iter().flatten() {
        let (Some(name), Some(version)) =
            (r.get("Name").and_then(Value::as_str), r.get("Version").and_then(Value::as_str))
        else {
            continue;
        };
        let pkg = Pkg {
            version: version.to_string(),
            depends: list(r, "Depends"),
            conflicts: list(r, "Conflicts"),
            provides: list(r, "Provides"),
        };
        out.insert(name.to_string(), pkg);
    }
    Ok(out)
}

/// `AUR_INFO?arg[]=a&arg[]=b`, for names pacman allows (no escaping needed).
pub fn info_url(names: &[&str]) -> String {
    let args: Vec<String> = names
        .iter()
        .filter(|n| n.bytes().all(|b| b.is_ascii_alphanumeric() || b"@._+-".contains(&b)))
        .map(|n| format!("arg[]={n}"))
        .collect();
    format!("{AUR_INFO}?{}", args.join("&"))
}

fn run(argv: &[&str]) -> Result<String, String> {
    let (prog, args) = argv.split_first().ok_or("empty command")?;
    let out = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("can't run {prog}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{prog} failed"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Look up the facts for these AUR updates; a failure is kept as the
/// problem (nothing is held back then).
pub fn gather(updates: &[Update]) -> Facts {
    if updates.is_empty() {
        return Facts::default();
    }
    let names: Vec<&str> = updates.iter().map(|u| u.name.as_str()).collect();
    let url = info_url(&names);
    let curl = ["curl", "-fsS", "--max-time", "20", "--proto", "=https", "--tlsv1.2", &url];
    let found = run(&curl).and_then(|j| candidates(&j)).and_then(|c| Ok((c, installed(&run(&["pacman", "-Qmi"])?))));
    match found {
        Ok((candidates, installed)) => Facts { installed, candidates, problem: None },
        Err(why) => Facts { problem: Some(why), ..Facts::default() },
    }
}
