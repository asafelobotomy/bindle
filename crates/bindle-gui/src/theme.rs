//! uQ's theme tokens (`~/.local/state/uq/theme/tokens.json`, written by uqd)
//! turned into what the QML `Theme` singleton reads. Without uQ, `present` is
//! false and the window follows the desktop's own Qt theme.

use std::path::PathBuf;
use std::time::SystemTime;

use serde_json::{Map, Value, json};

/// The colour tokens uQ themes define (see uq-core's `theme.rs`).
const COLORS: [&str; 12] = [
    "bg",
    "surface",
    "surface_alt",
    "overlay",
    "text",
    "text_muted",
    "accent",
    "accent_text",
    "border",
    "success",
    "warning",
    "error",
];

/// uqd rewrites tokens.json when the theme or variant changes: check once a
/// second and hand the new tokens (JSON) to `send` until it returns false
/// (the window is gone).
pub fn watch(path: Option<PathBuf>, send: impl Fn(String) -> bool + Send + 'static) {
    std::thread::Builder::new()
        .name("uq-theme".into())
        .spawn(move || {
            let mut seen = path.as_ref().and_then(modified);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let now = path.as_ref().and_then(modified);
                if now != seen {
                    seen = now;
                    if !send(load(path.as_ref()).to_string()) {
                        return;
                    }
                }
            }
        })
        .expect("spawning the theme watcher");
}

/// `$BINDLE_UQ_TOKENS` (screenshot tests), else uQ's state directory.
pub fn tokens_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("BINDLE_UQ_TOKENS") {
        return Some(PathBuf::from(p));
    }
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("uq/theme/tokens.json"))
}

pub fn modified(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn load(path: Option<&PathBuf>) -> Value {
    path.and_then(|p| std::fs::read_to_string(p).ok()).map_or_else(|| json!({ "present": false }), |t| parse(&t))
}

/// A token file with every colour as `#rrggbb`, or `present: false`: a half
/// theme would look worse than the desktop's own.
pub fn parse(text: &str) -> Value {
    let Ok(t) = serde_json::from_str::<Value>(text) else { return json!({ "present": false }) };
    let mut colors = Map::new();
    for key in COLORS {
        match t["colors"][key].as_str().filter(|c| is_hex(c)) {
            Some(c) => colors.insert(key.to_string(), json!(c)),
            None => return json!({ "present": false }),
        };
    }
    json!({
        "present": true,
        "name": t["name"].as_str().unwrap_or("uQ"),
        "variant": t["variant"].as_str().unwrap_or("dark"),
        "colors": colors,
        "radius": t["ui"]["radius"].as_u64().unwrap_or(8),
        "fontUi": t["fonts"]["ui"].as_str().unwrap_or(""),
        "fontMono": t["fonts"]["mono"].as_str().unwrap_or(""),
        "fontSize": t["fonts"]["size"].as_u64().unwrap_or(0),
    })
}

fn is_hex(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_uq_tokens() {
        let t = parse(include_str!("../tests/fixtures/rosewood-dark.json"));
        assert_eq!(t["present"], true);
        assert_eq!(t["colors"]["accent"], "#d4737f");
        assert_eq!(t["radius"], 10);
        assert_eq!(t["fontUi"], "Inter");
    }

    #[test]
    fn a_partial_or_broken_file_falls_back_to_the_desktop_theme() {
        assert_eq!(parse("not json")["present"], false);
        assert_eq!(parse(r##"{"colors":{"bg":"#000000"}}"##)["present"], false);
        let mut t: Value = serde_json::from_str(include_str!("../tests/fixtures/rosewood-dark.json")).unwrap();
        t["colors"]["accent"] = json!("red");
        assert_eq!(parse(&t.to_string())["present"], false);
        assert_eq!(load(None)["present"], false);
    }
}
