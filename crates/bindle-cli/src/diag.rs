//! How Bindle logs and crashes. A panic leaves a crash record for uQ's Problems page (uQ's
//! uq-diag). Under systemd (the update-check timer, first-login watch) a problem is written in
//! uQ's log format, `<4>bindle: [BINDLE-003] …`, so the journal files it at the right level; in
//! a terminal, stderr is for people and reads as before. The CLI writes these lines itself
//! rather than through tracing-subscriber, which would add half a megabyte to the binary.

use anyhow::Result;
use bindle_core::Facts;
use bindle_core::uq_diag::{self, Style};
use tracing::Level;

/// Whether stderr is systemd's journal (a timer or service runs this).
pub fn journal() -> bool {
    Style::detect() == Style::Journal
}

/// A problem with its code: `<p>bindle: msg` in the journal, `{plain}` in a terminal.
pub fn problem(level: Level, msg: &str, plain: &str) {
    if journal() {
        eprintln!("<{}>bindle: {msg}", uq_diag::priority(&level));
    } else {
        eprintln!("{plain}");
    }
}

/// Run `run`, with the crash record; under systemd a failure is logged as `[BINDLE-002]`.
pub fn main(run: fn() -> Result<()>) -> Result<()> {
    uq_diag::install_panic_hook("bindle");
    let r = run();
    if let Err(e) = &r
        && journal()
    {
        problem(Level::ERROR, &format!("[BINDLE-002] {e:#}"), "");
        std::process::exit(1);
    }
    r
}

pub fn warn_distro(facts: &Facts) {
    if !facts.is_arch_family() {
        eprintln!("warning: Bindle currently targets Arch Linux; detected `{}`. Plans may be wrong.", facts.distro_id);
    }
}
