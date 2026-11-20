//! `bindle config show|set|reset`: bindleup's preferences as typed keys, so
//! uQ Settings (and a script) can read and change them without editing TOML.
//! Reads the layers [`prefs`] reads; writes only the user's file, and
//! checks the whole file with [`Prefs::merge`] before it is saved.

use std::path::Path;

use serde::Serialize;
use toml::{Table, Value};

use crate::prefs::{Follow, Prefs};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Int,
    Choice,
}

/// One preference, as the page shows it.
pub struct Def {
    pub key: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
    pub max: i64,
    pub unit: &'static str,
    /// A choice's values and how the page names them.
    pub choices: &'static [(&'static str, &'static str)],
}

const fn int(
    key: &'static str,
    group: &'static str,
    label: &'static str,
    unit: &'static str,
    max: i64,
    help: &'static str,
) -> Def {
    Def { key, group, label, help, kind: Kind::Int, max, unit, choices: &[] }
}

pub const DEFS: &[Def] = &[
    int(
        "news_days",
        "Checks",
        "Arch news window",
        "days",
        3650,
        "Without a past upgrade in pacman.log, show this many days of news.",
    ),
    int(
        "mirror_max_age_days",
        "Checks",
        "Refresh mirrors after",
        "days",
        3650,
        "reflector runs when the mirror list is this old.",
    ),
    int(
        "firmware_refresh_days",
        "Checks",
        "Refresh firmware list after",
        "days",
        3650,
        "fwupd's list is refreshed by a real run when it is this old.",
    ),
    int(
        "space.root_mb",
        "Free space",
        "Needed on /",
        "MB",
        1_000_000,
        "Less free space holds the package upgrades back.",
    ),
    int(
        "space.boot_mb",
        "Free space",
        "Needed on /boot",
        "MB",
        1_000_000,
        "For a kernel, microcode or driver update on a separate /boot.",
    ),
    int(
        "space.efi_mb",
        "Free space",
        "Needed on the EFI partition",
        "MB",
        1_000_000,
        "Where boot images or the boot loader live there.",
    ),
    int(
        "battery_min_percent",
        "Battery",
        "Wait for power below",
        "%",
        100,
        "On battery below this charge, package upgrades wait.",
    ),
    int(
        "battery_max_packages",
        "Battery",
        "Wait for power above",
        "packages",
        100_000,
        "On battery, an upgrade this big (or with a kernel or driver) waits too.",
    ),
    Def {
        key: "releases.follow",
        group: "Apps",
        label: "Where pinned apps update from",
        help: "source: only copies Bindle installed. bindle: Bindle's release replaces any copy.",
        kind: Kind::Choice,
        max: 0,
        unit: "",
        choices: &[("source", "Only copies Bindle installed"), ("bindle", "Bindle's release replaces any copy")],
    },
    Def {
        key: "bar_icon",
        group: "uQ bar",
        label: "Show the bindleUP icon",
        help: "An icon in the Status island of uQ's bar. Click it to see what is ready and open the update window.",
        kind: Kind::Choice,
        max: 0,
        unit: "",
        choices: &[("updates", "When updates are ready"), ("always", "Always")],
    },
];

pub const FOLLOW_CHOICES: [&str; 2] = ["source", "bindle"];

fn path_get<'a>(t: &'a Table, key: &str) -> Option<&'a Value> {
    let mut parts = key.split('.');
    let mut cur = t.get(parts.next()?)?;
    for p in parts {
        cur = cur.as_table()?.get(p)?;
    }
    Some(cur)
}

fn path_set(t: &mut Table, key: &str, v: Value) {
    match key.split_once('.') {
        None => {
            t.insert(key.to_string(), v);
        }
        Some((head, rest)) => {
            let e = t.entry(head.to_string()).or_insert_with(|| Value::Table(Table::new()));
            if let Value::Table(sub) = e {
                path_set(sub, rest, v);
            }
        }
    }
}

fn path_unset(t: &mut Table, key: &str) {
    match key.split_once('.') {
        None => {
            t.remove(key);
        }
        Some((head, rest)) => {
            if let Some(Value::Table(sub)) = t.get_mut(head) {
                path_unset(sub, rest);
                if sub.is_empty() {
                    t.remove(head);
                }
            }
        }
    }
}

/// A per-app key: `releases.apps.<name>`.
fn app_name(key: &str) -> Option<&str> {
    key.strip_prefix("releases.apps.").filter(|n| !n.is_empty() && n.len() <= 64 && !n.contains(['.', '=', ' ', '"']))
}

#[derive(Serialize)]
pub struct Entry {
    pub key: String,
    pub group: String,
    pub label: String,
    pub help: String,
    pub kind: Kind,
    pub value: Value,
    pub default: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    pub unit: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// How the page names each of `choices`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choice_labels: Vec<String>,
    /// Which file sets it: default, system or user.
    pub source: &'static str,
}

#[derive(Serialize)]
pub struct AppEntry {
    pub app: String,
    pub value: String,
    pub source: &'static str,
}

#[derive(Serialize)]
pub struct Shown {
    /// Your file, where changes are written.
    pub path: String,
    pub keys: Vec<Entry>,
    pub apps: Vec<AppEntry>,
}

fn value_of(p: &Prefs, key: &str) -> Value {
    let n = |v: i64| Value::Integer(v);
    match key {
        "news_days" => n(p.news_days),
        "mirror_max_age_days" => n(p.mirror_max_age_days),
        "firmware_refresh_days" => n(p.firmware_refresh_days),
        "space.root_mb" => n(p.space.root_mb),
        "space.boot_mb" => n(p.space.boot_mb),
        "space.efi_mb" => n(p.space.efi_mb),
        "battery_min_percent" => n(p.battery_min_percent),
        "battery_max_packages" => n(p.battery_max_packages),
        "bar_icon" => Value::String(p.bar_icon.word().into()),
        _ => Value::String(word(p.follow).into()),
    }
}

pub fn word(f: Follow) -> &'static str {
    if f == Follow::Bindle { "bindle" } else { "source" }
}

fn table(text: Option<&str>) -> Table {
    text.and_then(|t| t.parse().ok()).unwrap_or_default()
}

/// Every preference with its value, default and where the value comes from.
/// `system` and `user` are the files' text (None: no file).
pub fn show(system: Option<&str>, user: Option<&str>, user_path: &str) -> Shown {
    let mut eff = Prefs::default();
    for t in [system, user].into_iter().flatten() {
        let _ = eff.merge(t);
    }
    let (st, ut) = (table(system), table(user));
    let source = |key: &str| {
        if path_get(&ut, key).is_some() {
            "user"
        } else if path_get(&st, key).is_some() {
            "system"
        } else {
            "default"
        }
    };
    let def = Prefs::default();
    let keys = DEFS
        .iter()
        .map(|d| Entry {
            key: d.key.into(),
            group: d.group.into(),
            label: d.label.into(),
            help: d.help.into(),
            kind: d.kind,
            value: value_of(&eff, d.key),
            default: value_of(&def, d.key),
            min: (d.kind == Kind::Int).then_some(0),
            max: (d.kind == Kind::Int).then_some(d.max),
            unit: d.unit.into(),
            choices: d.choices.iter().map(|(v, _)| v.to_string()).collect(),
            choice_labels: d.choices.iter().map(|(_, l)| l.to_string()).collect(),
            source: source(d.key),
        })
        .collect();
    let apps = eff
        .apps
        .iter()
        .map(|(a, f)| AppEntry {
            app: a.clone(),
            value: word(*f).into(),
            source: source(&format!("releases.apps.{a}")),
        })
        .collect();
    Shown { path: user_path.into(), keys, apps }
}

fn known(key: &str) -> bool {
    DEFS.iter().any(|d| d.key == key) || app_name(key).is_some()
}

fn parse_value(key: &str, raw: &str) -> Result<Value, String> {
    let choices: Option<Vec<&str>> = if app_name(key).is_some() {
        Some(FOLLOW_CHOICES.to_vec())
    } else {
        DEFS.iter()
            .find(|d| d.key == key && d.kind == Kind::Choice)
            .map(|d| d.choices.iter().map(|(v, _)| *v).collect())
    };
    if let Some(choices) = choices {
        return if choices.contains(&raw) {
            Ok(Value::String(raw.into()))
        } else {
            Err(format!("{key} is {}", choices.join(" or ")))
        };
    }
    raw.parse::<i64>().map(Value::Integer).map_err(|_| format!("{key} is a whole number"))
}

/// The user's file text after setting `key` (or removing it, `raw` None),
/// checked as a whole. An unreadable file is an error, never overwritten.
pub fn edited(user: Option<&str>, key: &str, raw: Option<&str>) -> Result<String, String> {
    if !known(key) {
        let keys: Vec<&str> = DEFS.iter().map(|d| d.key).collect();
        return Err(format!("no preference `{key}`; keys: {}, releases.apps.<app>", keys.join(", ")));
    }
    let mut t: Table = match user {
        Some(text) => {
            text.parse().map_err(|e| format!("your upgrade.toml can't be read ({e}); fix or remove it first"))?
        }
        None => Table::new(),
    };
    match raw {
        Some(raw) => path_set(&mut t, key, parse_value(key, raw)?),
        None => path_unset(&mut t, key),
    }
    let text = format!(
        "# bindleup preferences, written by `bindle config` (docs/UPGRADE.md)\n{}",
        toml::to_string(&t).map_err(|e| e.to_string())?
    );
    Prefs::default().merge(&text)?;
    Ok(text)
}

/// Write `text` to `path` through a temporary file.
pub fn save(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_value_and_source_per_layer() {
        let s = show(Some("news_days = 5\n"), Some("[space]\nroot_mb = 4000\n"), "/h/upgrade.toml");
        let get = |k: &str| s.keys.iter().find(|e| e.key == k).unwrap();
        assert_eq!((get("news_days").source, get("news_days").value.as_integer()), ("system", Some(5)));
        assert_eq!((get("space.root_mb").source, get("space.root_mb").value.as_integer()), ("user", Some(4000)));
        assert_eq!(get("space.boot_mb").source, "default");
        assert_eq!(get("releases.follow").choices, ["source", "bindle"]);
        assert_eq!(get("bar_icon").choices, ["updates", "always"]);
        assert_eq!(get("bar_icon").choice_labels.len(), 2);
        assert_eq!(get("bar_icon").value.as_str(), Some("updates"));
    }

    #[test]
    fn set_checks_the_value_and_keeps_other_keys() {
        let text = edited(Some("news_days = 9\n"), "space.boot_mb", Some("300")).unwrap();
        let mut p = Prefs::default();
        p.merge(&text).unwrap();
        assert_eq!((p.news_days, p.space.boot_mb), (9, 300));
        assert!(edited(None, "battery_min_percent", Some("101")).is_err());
        assert!(edited(None, "news_days", Some("many")).is_err());
        assert!(edited(None, "releases.follow", Some("aur")).is_err());
        assert!(edited(None, "bar_icon", Some("never")).unwrap_err().contains("updates or always"));
        let always = edited(None, "bar_icon", Some("always")).unwrap();
        let mut b = Prefs::default();
        b.merge(&always).unwrap();
        assert_eq!(b.bar_icon, crate::prefs::BarIcon::Always);
        assert!(edited(None, "bogus", Some("1")).unwrap_err().contains("no preference"));
        assert!(edited(Some("not toml ["), "news_days", Some("1")).unwrap_err().contains("can't be read"));
    }

    #[test]
    fn per_app_keys_set_and_reset() {
        let a = edited(None, "releases.apps.kyko", Some("bindle")).unwrap();
        let mut p = Prefs::default();
        p.merge(&a).unwrap();
        assert_eq!(p.follow("kyko"), Follow::Bindle);
        let b = edited(Some(&a), "releases.apps.kyko", None).unwrap();
        let mut q = Prefs::default();
        q.merge(&b).unwrap();
        assert!(q.apps.is_empty());
        assert!(edited(None, "releases.apps.", Some("bindle")).is_err());
    }
}
