//! Bindle core: catalog, detection and planning. No step here mutates the
//! system; execution lives in the CLI's privileged helper.

pub mod builtins;
pub mod catalog;
pub mod desktops;
pub mod detect;
pub mod install;
pub mod pacman_conf;
pub mod plan;

pub use catalog::Catalog;
pub use detect::Facts;
pub use plan::{Plan, Selection, Step};
