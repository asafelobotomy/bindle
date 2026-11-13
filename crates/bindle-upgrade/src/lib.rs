//! bindleupgrade: one guided run that updates the whole system.
//!
//! The run has three parts, kept apart so the middle one is pure:
//! 1. [`probe`] looks at the system without changing it (checkupdates,
//!    `paru -Qua`, Flatpak, fwupd, the Arch news feed, pacman.log, …) and
//!    fills a [`probe::Probe`]; [`parse`] and [`news`] turn command output
//!    into data.
//! 2. [`plan`] decides, from that data and the options, what each
//!    [`stage::Stage`] will do: run commands, report, wait for the news to be
//!    read, or nothing.
//! 3. [`run`] carries the plan out, as root through Bindle's helper or as the
//!    user, keeps going past a stage that fails, and returns one result per
//!    stage for [`report`] to print as a table.
//!
//! `--check` stops after step 2.

pub mod news;
pub mod parse;
pub mod plan;
pub mod prefs;
pub mod probe;
pub mod releases;
pub mod report;
pub mod run;
pub mod selfupdate;
pub mod setup;
pub mod stage;
pub mod system;

pub use plan::{Action, Options, StagePlan, Todo};
pub use stage::Stage;
