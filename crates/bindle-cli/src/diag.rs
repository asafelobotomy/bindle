//! How Bindle logs and crashes: uQ's log format (uq-diag) under systemd, plain stderr for people
//! in a terminal; a panic leaves a crash record for uQ's Problems page either way.

use anyhow::Result;
use bindle_core::Facts;
use bindle_core::uq_diag::{self, Style};

/// Whether stderr is systemd's journal (a timer or service runs this).
pub fn journal() -> bool {
    Style::detect() == Style::Journal
}

/// Run `run`. Under systemd (the update-check timer, first-login watch) a failure is also logged
/// as `[BINDLE-002]`, so uQ can explain it.
pub fn main(run: fn() -> Result<()>) -> Result<()> {
    if !journal() {
        uq_diag::install_panic_hook("bindle");
        return run();
    }
    uq_diag::init("bindle", "warn");
    run().inspect_err(|e| tracing::error!("[BINDLE-002] {e:#}"))
}

pub fn warn_distro(facts: &Facts) {
    if !facts.is_arch_family() {
        eprintln!("warning: Bindle currently targets Arch Linux; detected `{}`. Plans may be wrong.", facts.distro_id);
    }
}
