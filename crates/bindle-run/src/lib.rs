//! Bindle's apply engine, shared by the terminal and desktop frontends: the
//! orchestrator ([`apply`]), the root helper and its JSON protocol, snapshots,
//! release downloads, AUR builds, selection files, and the text every frontend
//! shows ([`present`]).

pub mod apply;
pub mod client;
pub mod helper;
pub mod net;
pub mod pacman_check;
pub mod present;
pub mod protocol;
pub mod record;
pub mod release;
pub mod selection;
pub mod self_install;
pub mod snapshot;
pub mod sys;
pub mod tty;
mod user;
pub mod whatsnew;
pub mod whatsnew_state;
