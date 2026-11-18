//! The external commands detection needs (pacman, lsblk, id, getent,
//! systemd-detect-virt). Each costs a process spawn, so they run side by side.

use std::collections::{BTreeMap, BTreeSet};

use super::{Drive, drives, hw, system};

pub(super) struct Probes {
    pub installed: BTreeSet<String>,
    pub groups: BTreeMap<String, BTreeSet<String>>,
    pub virt: String,
    pub drives: Vec<Drive>,
    pub user: String,
    pub user_groups: BTreeSet<String>,
    pub home: String,
}

pub(super) fn run(fstab: &str) -> Probes {
    std::thread::scope(|s| {
        let installed = s.spawn(system::installed_packages);
        let groups = s.spawn(system::package_groups);
        let virt = s.spawn(hw::virt);
        let drives = s.spawn(|| drives::detect(fstab));
        let account = s.spawn(|| {
            let user = system::current_user();
            let (groups, home) = std::thread::scope(|s| {
                let groups = s.spawn(|| system::user_groups(&user));
                (groups.join().unwrap_or_default(), system::user_home(&user))
            });
            (user, groups, home)
        });
        let (user, user_groups, home) = account.join().unwrap_or_default();
        Probes {
            installed: installed.join().unwrap_or_default(),
            groups: groups.join().unwrap_or_default(),
            virt: virt.join().unwrap_or_else(|_| "none".into()),
            drives: drives.join().unwrap_or_default(),
            user_groups,
            home,
            user,
        }
    })
}
