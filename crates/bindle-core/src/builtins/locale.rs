//! Settings that follow where the computer is and what language it speaks:
//! Wi-Fi's regulatory domain (from the timezone's country) and which CJK
//! font shapes come first (from the system language).

use super::Expansion;
use crate::catalog::FileSpec;
use crate::detect::Facts;

pub const REGDOM_FILE: &str = "/etc/modprobe.d/bindle-regdom.conf";
pub const CJK_FILE: &str = "/etc/fonts/conf.d/56-bindle-cjk.conf";

/// Wi-Fi's regulatory domain: without a country the radio keeps to the
/// "world" rules (fewer 5 and 6 GHz channels, lower power).
pub(super) fn regdom(f: &Facts) -> Expansion {
    let mut e = Expansion { packages: vec!["wireless-regdb".into()], ..Default::default() };
    match &f.extras.country {
        Some(cc) => e.files.push(FileSpec {
            path: REGDOM_FILE.into(),
            content: format!("# Managed by Bindle: Wi-Fi's rules for {cc} (from the timezone)\noptions cfg80211 ieee80211_regdom={cc}\n"),
            mode: 0o644,
        }),
        None => e.notes.push(
            "The country couldn't be told from the timezone: set Wi-Fi's region by hand in /etc/conf.d/wireless-regdom."
                .into(),
        ),
    }
    e
}

/// Noto's CJK font in the shapes the language uses: without a preference,
/// fontconfig shows Chinese text in Japanese glyph forms.
pub(super) fn cjk_fonts(f: &Facts) -> Expansion {
    let lang = f.lang.split(['.', '@']).next().unwrap_or("");
    let face = match lang {
        "zh_TW" | "zh_HK" | "zh_MO" => "Noto Sans CJK TC",
        l if l.starts_with("zh") => "Noto Sans CJK SC",
        l if l.starts_with("ja") => "Noto Sans CJK JP",
        l if l.starts_with("ko") => "Noto Sans CJK KR",
        _ => return Expansion::default(),
    };
    let serif = face.replace("Sans", "Serif");
    let mono = face.replace("Sans CJK", "Sans Mono CJK");
    let prefer = |family: &str, first: &str| {
        format!("  <alias>\n    <family>{family}</family>\n    <prefer><family>{first}</family></prefer>\n  </alias>\n")
    };
    let content = format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"fonts.dtd\">\n<!-- Managed by Bindle: {face} first for {lang} -->\n<fontconfig>\n{}{}{}</fontconfig>\n",
        prefer("sans-serif", face),
        prefer("serif", &serif),
        prefer("monospace", &mono),
    );
    Expansion {
        packages: vec!["noto-fonts-cjk".into()],
        files: vec![FileSpec { path: CJK_FILE.into(), content, mode: 0o644 }],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regdom_follows_the_country() {
        let mut f = Facts::default();
        f.extras.country = Some("GB".into());
        let e = regdom(&f);
        assert!(e.files[0].content.contains("ieee80211_regdom=GB"));
        f.extras.country = None;
        let e = regdom(&f);
        assert!(e.files.is_empty() && !e.notes.is_empty() && e.packages == ["wireless-regdb"]);
    }

    #[test]
    fn cjk_shapes_follow_the_language() {
        let with = |lang: &str| cjk_fonts(&Facts { lang: lang.into(), ..Default::default() });
        assert!(with("zh_TW.UTF-8").files[0].content.contains("<family>Noto Sans CJK TC</family>"));
        let sc = with("zh_CN.UTF-8").files[0].content.clone();
        assert!(sc.contains("Noto Serif CJK SC") && sc.contains("Noto Sans Mono CJK SC"));
        assert!(with("ja_JP.UTF-8").files[0].content.contains("CJK JP"));
        assert!(with("en_GB.UTF-8").files.is_empty(), "other languages: nothing to prefer");
    }
}
