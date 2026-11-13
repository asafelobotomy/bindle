//! bindo: Bindle's recipe runner, in the spirit of Universal Blue's `ujust`.
//!
//! A recipe is a short, named, documented system task: a POSIX `sh` script
//! with typed arguments, kept in TOML ([`recipe`]). Recipes ship inside the
//! binary and can be added or overridden from directories ([`book`]).
//! Arguments are checked before anything runs ([`args`]) and reach the
//! script as positional parameters, never pasted into its text. Admin
//! recipes run through polkit (pkexec, else run0), never sudo ([`exec`]).
//! What the user reads is built as plain lines ([`show`]).

pub mod args;
pub mod book;
pub mod exec;
pub mod recipe;
pub mod show;

pub use book::Book;
pub use recipe::{Arg, Kind, Origin, Recipe};
