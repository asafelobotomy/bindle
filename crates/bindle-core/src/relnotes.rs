//! Release notes: the CHANGELOG section of a version, cut down to a few
//! short highlights for the "What's new in Bindle X.Y.Z" popup. Pure: the
//! changelog is embedded at build time (`CHANGELOG`), so the AppImage and
//! the installed binary carry their own notes.

use serde::Serialize;

/// The project's CHANGELOG.md as of this build.
pub const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// Highlights shown; the rest are only counted (`ReleaseNotes::more`).
pub const MAX_HIGHLIGHTS: usize = 4;
/// Longest detail text, in characters (cut at a word, with an ellipsis).
pub const MAX_DETAIL: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Highlight {
    /// The bold lead of the bullet ("bindleUP checks unified kernel images").
    pub title: String,
    /// What follows it, plain text, bounded to `MAX_DETAIL`.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseNotes {
    pub version: String,
    pub date: Option<String>,
    pub highlights: Vec<Highlight>,
    /// Bullets in the section that aren't shown.
    pub more: usize,
}

impl ReleaseNotes {
    /// "What's new in Bindle 0.1.17"
    pub fn title(&self) -> String {
        format!("What's new in Bindle {}", self.version)
    }

    /// The highlights as one short text (a notification body).
    pub fn summary(&self) -> String {
        let mut s = self.highlights.iter().map(|h| h.title.as_str()).collect::<Vec<_>>().join(" · ");
        if self.more > 0 {
            s.push_str(&format!(" · and {} more", self.more));
        }
        s
    }
}

/// The notes of `version` in `changelog`; None when it has no section, or
/// the section lists nothing (`## Unreleased` on a development build).
pub fn parse(changelog: &str, version: &str) -> Option<ReleaseNotes> {
    let version = version.trim_start_matches('v');
    let mut date = None;
    let mut bullets: Vec<String> = vec![];
    let mut inside = false;
    for line in changelog.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            if inside {
                break;
            }
            let mut words = head.splitn(2, char::is_whitespace);
            if words.next() == Some(version) {
                inside = true;
                date = words.next().map(|d| d.trim_start_matches(['—', '–', '-', ' ']).trim().to_string());
                date = date.filter(|d| !d.is_empty());
            }
        } else if inside {
            if let Some(b) = line.strip_prefix("- ") {
                bullets.push(b.trim().to_string());
            } else if line.starts_with("  ")
                && !line.trim().is_empty()
                && let Some(last) = bullets.last_mut()
            {
                last.push(' ');
                last.push_str(line.trim());
            }
        }
    }
    if bullets.is_empty() {
        return None;
    }
    let more = bullets.len().saturating_sub(MAX_HIGHLIGHTS);
    let highlights = bullets.iter().take(MAX_HIGHLIGHTS).map(|b| highlight(b)).collect();
    Some(ReleaseNotes { version: version.to_string(), date, highlights, more })
}

/// The notes of the running version.
pub fn current() -> Option<ReleaseNotes> {
    parse(CHANGELOG, env!("CARGO_PKG_VERSION"))
}

/// One bullet: its bold lead (else its first sentence) and the rest.
fn highlight(bullet: &str) -> Highlight {
    let (title, rest) = match bullet.strip_prefix("**").and_then(|b| b.split_once("**")) {
        Some((t, r)) => (t.to_string(), r.to_string()),
        None => {
            let end = bullet.find(". ").map_or(bullet.len(), |i| i + 1);
            (bullet[..end].to_string(), bullet[end..].to_string())
        }
    };
    Highlight { title: plain(&title).trim_end_matches('.').to_string(), detail: shorten(&plain(&rest), MAX_DETAIL) }
}

/// Without the markdown marks that mean nothing in a popup.
fn plain(s: &str) -> String {
    s.replace(['`', '*'], "").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// At most `max` characters, cut at a word, with "…".
fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([',', ';', ':', '.', ' ']))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "# Changelog\n\n## Unreleased\n\n## 0.2.0 — 2026-12-01\n\n\
- **Big thing.** It does a lot of things for `you`, and\n  goes on over two lines.\n\
- No bold lead here. The rest follows.\n- **Third** third.\n- **Fourth** four.\n- **Fifth** five.\n\
\n## 0.1.9 — 2026-11-01\n\n- **Old.** old.\n";

    #[test]
    fn it_reads_the_section_of_one_version() {
        let n = parse(LOG, "0.2.0").unwrap();
        assert_eq!(n.date.as_deref(), Some("2026-12-01"));
        assert_eq!(n.title(), "What's new in Bindle 0.2.0");
        assert_eq!(n.highlights.len(), MAX_HIGHLIGHTS);
        assert_eq!(n.more, 1);
        assert_eq!(n.highlights[0].title, "Big thing");
        assert_eq!(n.highlights[0].detail, "It does a lot of things for you, and goes on over two lines.");
        assert_eq!(parse(LOG, "v0.1.9").unwrap().highlights[0].title, "Old");
    }

    #[test]
    fn a_bullet_without_a_bold_lead_uses_its_first_sentence() {
        let h = &parse(LOG, "0.2.0").unwrap().highlights[1];
        assert_eq!((h.title.as_str(), h.detail.as_str()), ("No bold lead here", "The rest follows."));
    }

    #[test]
    fn nothing_to_show_is_none() {
        assert_eq!(parse(LOG, "0.3.0"), None, "no such version");
        assert_eq!(parse(LOG, "0.1"), None, "not a prefix match");
        assert_eq!(parse("## Unreleased\n\n## 0.1.0 — x\n", "0.1.0"), None, "an empty section");
        assert_eq!(parse("", "0.1.0"), None);
    }

    #[test]
    fn detail_is_bounded_at_a_word() {
        let long = format!("- **T.** {}\n", "word ".repeat(100));
        let n = parse(&format!("## 1.0.0 — d\n{long}"), "1.0.0").unwrap();
        let d = &n.highlights[0].detail;
        assert!(d.chars().count() <= MAX_DETAIL + 1 && d.ends_with("word…"), "{d}");
    }

    #[test]
    fn the_summary_lists_titles_and_counts_the_rest() {
        let n = parse(LOG, "0.2.0").unwrap();
        assert_eq!(n.summary(), "Big thing · No bold lead here · Third · Fourth · and 1 more");
    }

    #[test]
    fn the_shipped_changelog_has_notes_for_released_versions() {
        let n = parse(CHANGELOG, "0.1.16").expect("0.1.16 is in CHANGELOG.md");
        assert!(!n.highlights[0].title.is_empty() && n.highlights.len() <= MAX_HIGHLIGHTS);
    }
}
