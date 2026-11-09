//! Theme icons recoloured as flat silhouettes, for `Glyph.qml`. Outside
//! Plasma, Kirigami draws Breeze's dark glyphs as they are (invisible on a
//! dark theme), and shader effects need a GPU; rewriting the SVG's colours
//! works everywhere, offscreen screenshots included.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Themes to look in, in order: Breeze's names are the ones the views use.
const THEMES: &[&str] = &["breeze", "breeze-dark", "Papirus", "hicolor"];

/// The icon `name` from the first theme that has it, every colour replaced
/// by `color` (`#rrggbb`), as a `data:` URL; empty when no theme has it.
pub fn url(name: &str, color: &str) -> String {
    static CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
    let key = format!("{name}{color}");
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(u) = cache.get(&key) {
        return u.clone();
    }
    let u = find(name, &icon_dirs())
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|svg| format!("data:image/svg+xml;utf8,{}", percent_encode(&recolor(&svg, color))))
        .unwrap_or_default();
    cache.insert(key, u.clone());
    u
}

/// `$XDG_DATA_HOME/icons` and each `$XDG_DATA_DIRS/icons`.
fn icon_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share")));
    let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    home.into_iter()
        .chain(dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from))
        .map(|d| d.join("icons"))
        .collect()
}

/// `<dir>/<theme>/<context>/<size>/<name>.svg`, smallest size first (the
/// glyph-like variant); Papirus nests the other way round (`<size>/<context>`).
fn find(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let file = format!("{name}.svg");
    for theme in THEMES {
        for dir in dirs {
            let root = dir.join(theme);
            let Ok(top) = std::fs::read_dir(&root) else { continue };
            let mut hits: Vec<(u32, PathBuf)> = Vec::new();
            for a in top.flatten() {
                let Ok(sub) = std::fs::read_dir(a.path()) else { continue };
                for b in sub.flatten() {
                    let candidate = b.path().join(&file);
                    if candidate.is_file() {
                        let size = |p: &Path| p.file_name()?.to_str()?.split('x').next()?.parse::<u32>().ok();
                        let n = size(&b.path()).or_else(|| size(&a.path())).unwrap_or(u32::MAX);
                        hits.push((n, candidate));
                    }
                }
            }
            if let Some((_, p)) = hits.into_iter().min() {
                return Some(p);
            }
        }
    }
    None
}

/// Every `#rgb`/`#rrggbb` colour and `currentColor` becomes `color`, so the
/// icon draws as one flat shape (opacity and shape are kept).
fn recolor(svg: &str, color: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(i) = rest.find('#') {
        out.push_str(&rest[..i]);
        let hex = rest[i + 1..].bytes().take_while(u8::is_ascii_hexdigit).count();
        // `url(#id)` references have letters past the hex digits: leave them.
        let next = rest[i + 1 + hex..].chars().next();
        let ends = next.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
        if (hex == 3 || hex == 6) && ends {
            out.push_str(color);
            rest = &rest[i + 1 + hex..];
        } else {
            out.push('#');
            rest = &rest[i + 1..];
        }
    }
    out.push_str(rest);
    out.replace("currentColor", color)
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3 / 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b' ' | b'=' | b':' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out.replace(' ', "%20")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recolors_fills_and_current_color_but_not_references() {
        let svg = r##"<style>.ColorScheme-Text{color:#232629;}</style><path fill="currentColor" stroke="#fff" fill-opacity=".5" style="fill:url(#grad1)"/><stop stop-color="#abcdef"/>"##;
        let out = recolor(svg, "#ff0000");
        assert!(!out.contains("#232629") && !out.contains("#fff\"") && !out.contains("#abcdef"), "{out}");
        assert!(out.contains("url(#grad1)") && out.contains("fill=\"#ff0000\""), "{out}");
    }

    #[test]
    fn finds_the_smallest_size_of_an_icon() {
        let dir = std::env::temp_dir().join(format!("bindle-glyph-{}", std::process::id()));
        for size in ["16", "22"] {
            std::fs::create_dir_all(dir.join(format!("breeze/actions/{size}"))).unwrap();
            std::fs::write(dir.join(format!("breeze/actions/{size}/go-next.svg")), "<svg/>").unwrap();
        }
        let hit = find("go-next", std::slice::from_ref(&dir)).unwrap();
        assert!(hit.ends_with("breeze/actions/16/go-next.svg"), "{}", hit.display());
        assert_eq!(find("no-such-icon", std::slice::from_ref(&dir)), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn encodes_svg_for_a_data_url() {
        assert_eq!(percent_encode("<a b=\"#c\"/>"), "%3Ca%20b=%22%23c%22/%3E");
    }
}
