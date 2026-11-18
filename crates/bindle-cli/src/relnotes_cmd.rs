//! The release notes ("What's new in Bindle X.Y.Z") in the terminal and as a
//! notification. Shown once per version, after Bindle was updated: see
//! `bindle_run::relnotes`. "Open What's new" is the existing per-item review.

use anyhow::Result;
use bindle_run::relnotes::{self, ReleaseNotes};

/// The notes as lines: a title, each highlight, how many more there are.
pub fn lines(n: &ReleaseNotes) -> Vec<String> {
    let mut out =
        vec![String::new(), format!("{}{}", n.title(), n.date.as_ref().map_or(String::new(), |d| format!(" ({d})")))];
    for h in &n.highlights {
        out.push(format!("  • {}", h.title));
        if !h.detail.is_empty() {
            out.push(format!("    {}", h.detail));
        }
    }
    if n.more > 0 {
        out.push(format!("  … and {} more (CHANGELOG.md)", n.more));
    }
    out.push(String::new());
    out
}

/// Print the pending notes (if any) and mark them seen. True when shown.
pub fn show_pending() -> bool {
    let Some(n) = relnotes::pending() else { return false };
    lines(&n).iter().for_each(|l| println!("{l}"));
    relnotes::dismiss();
    true
}

/// `bindle whats-new --notify`: one notification with the release summary
/// (when there are notes to show) and how many items wait in What's new.
/// "Open What's new" starts the review; either button marks the notes seen,
/// a notification nobody answered leaves them for the window or terminal.
pub fn notify(items: usize) -> Result<()> {
    let notes = relnotes::pending();
    if notes.is_none() && items == 0 {
        return Ok(());
    }
    let title = notes
        .as_ref()
        .map_or_else(|| format!("What's new in Bindle {}", bindle_run::whatsnew::VERSION), ReleaseNotes::title);
    let review = match items {
        0 => String::new(),
        n => format!("{n} to review: new features, changes and clean-ups."),
    };
    let body = match &notes {
        Some(n) => format!("{} {review}", n.summary()).trim().to_string(),
        None => review,
    };
    let buttons: &[(&str, &str)] =
        if items > 0 { &[("review", "Open What's new"), ("later", "Later")] } else { &[("later", "Dismiss")] };
    let answer = crate::notify::show(&title, &body, buttons)?;
    if answer.is_some() && notes.is_some() {
        relnotes::dismiss();
    }
    if answer.as_deref() == Some("review") {
        crate::gui::whats_new()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notes_print_as_a_short_list() {
        let n = bindle_core::relnotes::parse("## 1.0.0 — 2026-12-01\n- **A.** one\n- **B** two\n", "1.0.0").unwrap();
        assert_eq!(
            lines(&n),
            ["", "What's new in Bindle 1.0.0 (2026-12-01)", "  • A", "    one", "  • B", "    two", ""]
        );
    }
}
