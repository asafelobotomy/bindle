//! Bindle core: catalog, detection and planning. No step here mutates the
//! system; execution lives in the CLI's privileged helper.

pub mod builtins;
pub mod catalog;
pub mod desktops;
pub mod detect;
pub mod fstab;
pub mod initramfs;
pub mod install;
pub mod pacman_conf;
pub mod plan;
pub mod restart;
pub mod whatsnew;

pub use catalog::Catalog;
pub use detect::Facts;
pub use plan::{Plan, Selection, Step};

/// Order of Bindle versions (`0.1.9`, `0.2.0-rc.1`): numbers compared as
/// numbers; a pre-release sorts before its release.
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |v: &str| {
        let (core, pre) = v.trim_start_matches('v').split_once('-').unwrap_or((v.trim_start_matches('v'), ""));
        let nums: Vec<u64> = core.split('.').map(|n| n.parse().unwrap_or(0)).collect();
        (nums, pre.to_string())
    };
    let ((na, pa), (nb, pb)) = (split(a), split(b));
    na.cmp(&nb).then_with(|| match (pa.is_empty(), pb.is_empty()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => pa.cmp(&pb),
    })
}
