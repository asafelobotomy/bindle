//! The snapper settings uQ Settings' Restore page and `bindle snapshot set`
//! may change: automatic hourly/daily snapshots and how many of each, and
//! how many pacman snapshots to keep. Each value is checked here before the
//! root helper hands it to `snapper set-config`; nothing else in the config
//! can be changed this way.

use serde_json::{Map, Value};

/// The keys that can be read and set, in the order they are shown.
pub const KEYS: &[&str] = &[
    "TIMELINE_CREATE",
    "TIMELINE_LIMIT_HOURLY",
    "TIMELINE_LIMIT_DAILY",
    "TIMELINE_LIMIT_WEEKLY",
    "TIMELINE_LIMIT_MONTHLY",
    "TIMELINE_LIMIT_QUARTERLY",
    "TIMELINE_LIMIT_YEARLY",
    "NUMBER_LIMIT",
    "NUMBER_LIMIT_IMPORTANT",
];

/// Why `value` can't be given to `key`, or `None` when it can. Switches
/// take yes/no; limits a number up to 999, or a range like 2-10 for
/// NUMBER_LIMIT (snapper keeps fewer when space runs low).
pub fn check(key: &str, value: &str) -> Option<String> {
    let number = |v: &str| !v.is_empty() && v.len() <= 3 && v.bytes().all(|b| b.is_ascii_digit());
    let ok = match key {
        "TIMELINE_CREATE" => matches!(value, "yes" | "no"),
        "NUMBER_LIMIT" | "NUMBER_LIMIT_IMPORTANT" => {
            number(value) || value.split_once('-').is_some_and(|(a, b)| number(a) && number(b))
        }
        k if KEYS.contains(&k) => number(value),
        _ => return Some(format!("{key} isn't one of the snapshot settings Bindle changes")),
    };
    (!ok).then(|| format!("{value:?} isn't a value for {key}"))
}

/// The settings above from `snapper --jsonout -c root get-config`
/// (`{"KEY": "value", ...}`; older snapper prints `{"root": {...}}`).
/// Keys snapper didn't print are left out.
pub fn parse(json: &str) -> Result<Map<String, Value>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("snapper's settings aren't readable: {e}"))?;
    let all = match v.get("root") {
        Some(Value::Object(m)) => m.clone(),
        _ => v.as_object().cloned().ok_or("snapper's settings aren't an object")?,
    };
    Ok(KEYS.iter().filter_map(|k| all.get(*k).map(|v| (k.to_string(), v.clone()))).collect())
}

/// Settings in snapper's plain `get-config` table (`KEY │ value`), for
/// snapper versions without --jsonout there.
pub fn parse_table(text: &str) -> Map<String, Value> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('│').or_else(|| l.split_once('|'))?;
            let k = k.trim();
            KEYS.contains(&k).then(|| (k.to_string(), Value::String(v.trim().to_string())))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_keys_and_plain_values() {
        assert_eq!(check("TIMELINE_CREATE", "yes"), None);
        assert!(check("TIMELINE_CREATE", "1").is_some());
        assert_eq!(check("TIMELINE_LIMIT_DAILY", "7"), None);
        assert!(check("TIMELINE_LIMIT_DAILY", "1000").is_some());
        assert!(check("TIMELINE_LIMIT_DAILY", "").is_some());
        assert!(check("TIMELINE_LIMIT_DAILY", "2-10").is_some());
        assert_eq!(check("NUMBER_LIMIT", "2-10"), None);
        assert!(check("NUMBER_LIMIT", "2-").is_some());
        assert!(check("SUBVOLUME", "/").is_some());
        assert!(check("ALLOW_USERS", "me").is_some());
        assert!(check("TIMELINE_LIMIT_DAILY", "7; rm").is_some());
    }

    #[test]
    fn reads_json_and_tables() {
        let s = parse(r#"{"SUBVOLUME": "/", "TIMELINE_CREATE": "yes", "NUMBER_LIMIT": "50"}"#).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s["TIMELINE_CREATE"], "yes");
        assert_eq!(parse(r#"{"root": {"NUMBER_LIMIT": "10"}}"#).unwrap()["NUMBER_LIMIT"], "10");
        assert!(parse("nope").is_err());
        let t = parse_table(
            "Key                    │ Value\n───────────────────────┼──────\nNUMBER_LIMIT           │ 50\nSUBVOLUME │ /\nTIMELINE_CREATE        │ no\n",
        );
        assert_eq!(t.len(), 2);
        assert_eq!(t["TIMELINE_CREATE"], "no");
    }
}
