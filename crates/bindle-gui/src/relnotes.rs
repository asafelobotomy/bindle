//! The release-notes popup's data, for both windows: the notes of the
//! running Bindle when they are due (once per version, after an update).

use bindle_run::relnotes::ReleaseNotes;
use serde_json::{Value, json};

/// What the popup reads (`ReleaseNotesDialog.qml`).
pub fn json(n: &ReleaseNotes) -> Value {
    json!({
        "state": "show",
        "title": n.title(),
        "date": n.date,
        "highlights": n.highlights,
        "more": n.more,
    })
}

/// Nothing to show.
pub fn none() -> Value {
    json!({ "state": "none" })
}

/// The notes due now: only in a real run (not screenshots or a sandbox).
pub fn due(real: bool) -> Value {
    match real.then(bindle_run::relnotes::pending).flatten() {
        Some(n) => json(&n),
        None => none(),
    }
}

/// Made-up notes for the screenshots.
pub fn sample() -> Value {
    let log = "## 0.2.0 — 2026-12-01\n\
- **Release notes after an update.** A short list of what the release brings, shown once, with a way into What's new.\n\
- **bindleUP checks unified kernel images.** The Reboot stage reads the kernel inside each image and says when one is stale.\n\
- **A retired module no longer breaks a saved selection.** Commands that read it skip the retired ids.\n\
- **Newer appOFholding and emobie.** New pins, with their own settings pages.\n\
- **Boot guard.** A pacman hook refuses a kernel change while /boot isn't mounted.\n";
    json(&bindle_core::relnotes::parse(log, "0.2.0").expect("the sample parses"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_shows_four_highlights_and_the_rest_counted() {
        let s = sample();
        assert_eq!(s["state"], "show");
        assert_eq!(s["title"], "What's new in Bindle 0.2.0");
        assert_eq!(s["highlights"].as_array().unwrap().len(), 4);
        assert_eq!(s["more"], 1);
    }

    #[test]
    fn nothing_is_due_outside_a_real_run() {
        assert_eq!(due(false)["state"], "none");
    }
}
