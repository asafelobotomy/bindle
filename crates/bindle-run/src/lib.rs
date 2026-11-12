//! Bindle's apply engine, shared by the terminal and desktop frontends: the
//! orchestrator ([`apply`]), the root helper and its JSON protocol, snapshots,
//! release downloads, AUR builds, selection files, and the text every frontend
//! shows ([`present`]).

pub mod apply;
pub mod client;
pub mod helper;
pub mod net;
pub mod present;
pub mod protocol;
mod release;
pub mod selection;
pub mod snapshot;
pub mod sys;
mod user;
