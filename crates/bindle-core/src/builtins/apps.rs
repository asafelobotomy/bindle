//! Everyday apps for desktops that ship none, and spelling dictionaries for
//! the system language.

use super::Expansion;
use crate::desktops;
use crate::detect::Facts;

/// Every repo package this file can add (checked by scripts/check-packages.py).
pub const PACKAGES: &[&str] = &[
    "dolphin",
    "ark",
    "gwenview",
    "okular",
    "kcalc",
    "partitionmanager",
    "kdegraphics-thumbnailers",
    "ffmpegthumbs",
    "nautilus",
    "file-roller",
    "loupe",
    "papers",
    "gnome-calculator",
    "gnome-disk-utility",
    "gnome-system-monitor",
    "ffmpegthumbnailer",
    "gvfs",
    "hunspell",
    "hunspell-en_us",
    "hunspell-en_gb",
    "hunspell-en_au",
    "hunspell-en_ca",
    "hunspell-de",
    "hunspell-el",
    "hunspell-es_any",
    "hunspell-fr-comprehensive",
    "hunspell-he",
    "hunspell-hu",
    "hunspell-it",
    "hunspell-nl",
    "hunspell-pl",
    "hunspell-ro",
    "hunspell-ru",
];

/// File manager, archives, image and PDF viewers, calculator and a disk tool,
/// in the desktop's toolkit. KDE has no light system monitor, so Qt has none.
const QT: &[&str] =
    &["dolphin", "ark", "gwenview", "okular", "kcalc", "partitionmanager", "kdegraphics-thumbnailers", "ffmpegthumbs"];
const GTK: &[&str] = &[
    "nautilus",
    "file-roller",
    "loupe",
    "papers",
    "gnome-calculator",
    "gnome-disk-utility",
    "gnome-system-monitor",
    "ffmpegthumbnailer",
    "gvfs",
];

/// The everyday set for each detected desktop that ships none of its own.
pub(super) fn everyday(f: &Facts) -> Expansion {
    let data = desktops::all().expect("desktops.toml is embedded and validated with the catalog");
    let mut e = Expansion::default();
    for d in f.desktops.iter().filter_map(|id| desktops::by_id(&data, id)) {
        let set = match d.everyday.as_deref() {
            Some("qt") => QT,
            Some("gtk") => GTK,
            _ => continue,
        };
        for p in set {
            if !e.packages.iter().any(|x| x == p) {
                e.packages.push((*p).into());
            }
        }
    }
    e
}

/// Dictionaries for `LANG` (Firefox, Thunderbird and LibreOffice use them).
/// English stays alongside another language: most people type some.
pub(super) fn spellcheck(f: &Facts) -> Expansion {
    let lang = f.lang.split(['.', '@']).next().unwrap_or("");
    let (language, region) = lang.split_once('_').unwrap_or((lang, ""));
    let dict = match (language, region) {
        ("en", "GB" | "IE" | "NZ" | "ZA" | "IN") => "hunspell-en_gb",
        ("en", "AU") => "hunspell-en_au",
        ("en", "CA") => "hunspell-en_ca",
        ("en", _) | ("C" | "POSIX" | "", _) => "hunspell-en_us",
        ("es", _) => "hunspell-es_any",
        ("fr", _) => "hunspell-fr-comprehensive",
        ("de" | "el" | "he" | "hu" | "it" | "nl" | "pl" | "ro" | "ru", _) => {
            return Expansion {
                packages: vec!["hunspell".into(), format!("hunspell-{language}"), "hunspell-en_us".into()],
                ..Default::default()
            };
        }
        _ => {
            return Expansion {
                packages: vec!["hunspell".into(), "hunspell-en_us".into()],
                notes: vec![format!("Arch has no dictionary for {lang}; English is installed.")],
                ..Default::default()
            };
        }
    };
    let mut packages = vec!["hunspell".to_string(), dict.into()];
    if language != "en" && !packages.iter().any(|p| p == "hunspell-en_us") {
        packages.push("hunspell-en_us".into());
    }
    Expansion { packages, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(desktops: &[&str], lang: &str) -> Facts {
        Facts { desktops: desktops.iter().map(|d| d.to_string()).collect(), lang: lang.into(), ..Default::default() }
    }

    #[test]
    fn everyday_apps_only_for_desktops_without_their_own() {
        let sway = everyday(&facts(&["sway"], ""));
        assert!(sway.packages.contains(&"nautilus".to_string()), "{:?}", sway.packages);
        let uq = everyday(&facts(&["uq"], ""));
        assert!(uq.packages.contains(&"gwenview".to_string()) && !uq.packages.contains(&"nautilus".to_string()));
        assert!(everyday(&facts(&["plasma"], "")).packages.is_empty());
        assert!(everyday(&facts(&["gnome"], "")).packages.is_empty());
        // Two bare desktops of one toolkit: each package once.
        let two = everyday(&facts(&["sway", "i3"], ""));
        assert_eq!(two.packages.len(), GTK.len());
    }

    #[test]
    fn dictionaries_follow_the_system_language() {
        let pk = |lang: &str| spellcheck(&facts(&[], lang)).packages;
        assert_eq!(pk("en_GB.UTF-8"), ["hunspell", "hunspell-en_gb"]);
        assert_eq!(pk("en_US.UTF-8"), ["hunspell", "hunspell-en_us"]);
        assert_eq!(pk(""), ["hunspell", "hunspell-en_us"]);
        assert_eq!(pk("de_DE.UTF-8"), ["hunspell", "hunspell-de", "hunspell-en_us"]);
        assert_eq!(pk("es_MX.UTF-8"), ["hunspell", "hunspell-es_any", "hunspell-en_us"]);
        let ja = spellcheck(&facts(&[], "ja_JP.UTF-8"));
        assert_eq!(ja.packages, ["hunspell", "hunspell-en_us"]);
        assert!(ja.notes[0].contains("ja_JP"));
    }
}
