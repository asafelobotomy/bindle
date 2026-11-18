//! The "What's new in Bindle X.Y.Z" release notes, once per version: the
//! notes of the running Bindle (`bindle_core::relnotes`) when it was updated
//! since they were last seen. A dismissed version stays dismissed; a fresh
//! install never sees them.

pub use bindle_core::relnotes::ReleaseNotes;

use crate::whatsnew_state::{self, Notes};

/// The notes to show now, if any. On a fresh install this also records the
/// running version as seen, so the first update after it shows its notes.
pub fn pending() -> Option<ReleaseNotes> {
    let notes = bindle_core::relnotes::current()?;
    pending_with(&whatsnew_state::load(), crate::whatsnew::saved().is_some(), notes, || {
        let _ = whatsnew_state::mark_notes_seen();
    })
}

/// `pending` over given inputs: `baseline` runs when this is a fresh install.
fn pending_with(
    state: &whatsnew_state::State,
    set_up: bool,
    notes: ReleaseNotes,
    baseline: impl FnOnce(),
) -> Option<ReleaseNotes> {
    match state.notes(&notes.version, set_up) {
        Notes::Show => Some(notes),
        Notes::Baseline => {
            baseline();
            None
        }
        Notes::Quiet => None,
    }
}

/// Done with the notes: shown and answered (either button), or declined.
pub fn dismiss() {
    let _ = whatsnew_state::mark_notes_seen();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whatsnew_state::State;
    use std::cell::Cell;

    fn notes() -> ReleaseNotes {
        bindle_core::relnotes::parse("## 1.0.0 — d\n- **A.** b\n", "1.0.0").unwrap()
    }

    #[test]
    fn an_update_shows_the_notes_and_a_fresh_install_only_records_the_version() {
        let seen = State { notes_seen: Some("0.9.0".into()), ..Default::default() };
        let recorded = Cell::new(false);
        assert!(pending_with(&seen, true, notes(), || recorded.set(true)).is_some());
        assert!(!recorded.get());
        assert!(pending_with(&State::default(), false, notes(), || recorded.set(true)).is_none());
        assert!(recorded.get(), "baseline written so the next update shows its notes");
        let done = State { notes_seen: Some("1.0.0".into()), ..Default::default() };
        assert!(pending_with(&done, true, notes(), || {}).is_none());
    }
}
