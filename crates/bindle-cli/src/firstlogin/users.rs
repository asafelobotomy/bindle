//! Which users have finished first-login setup. Their homes are private
//! (0700), so only root can tell ([`unfinished`], behind `first-login
//! remove --if-all-done`); a user's own session reads only /etc/passwd.

use std::fs;
use std::path::{Path, PathBuf};

use crate::apply::sys::host;

/// Home directories of the regular login accounts in a passwd file.
fn regular_homes(passwd: &str, uid_min: u32) -> Vec<PathBuf> {
    passwd
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            let (uid, home, shell) = (f.get(2)?.parse().ok()?, *f.get(5)?, *f.get(6).unwrap_or(&""));
            let login = !shell.ends_with("/nologin") && !shell.ends_with("/false");
            (crate::session::is_regular_uid(uid, uid_min) && login && !home.is_empty()).then(|| PathBuf::from(home))
        })
        .collect()
}

fn homes(root: &Path) -> Vec<PathBuf> {
    let read = |p: &str| fs::read_to_string(host(root, p)).unwrap_or_default();
    regular_homes(&read("/etc/passwd"), crate::session::uid_min(&read("/etc/login.defs")))
}

/// As root: the homes of regular users who haven't finished (or dismissed)
/// first-login setup.
pub fn unfinished(root: &Path) -> Vec<PathBuf> {
    homes(root)
        .into_iter()
        .filter(|h| !host(root, &h.to_string_lossy()).join(".local/state/bindle/first-login-done").exists())
        .collect()
}

/// The only regular account here is the one living in `home` (passwd only).
pub fn only_user(root: &Path, home: &Path) -> bool {
    homes(root) == [home]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::sys::create_dirs;
    use crate::firstlogin::{hooks_installed, install, remove};

    #[test]
    fn last_user_done() {
        let root = crate::apply::sys::test_dir("firstlogin-users");
        let passwd = "root:x:0:0::/root:/usr/bin/bash\nsddm:x:964:964::/var/lib/sddm:/usr/bin/nologin\n\
                      ann:x:1000:1000::/home/ann:/usr/bin/zsh\nbob:x:1001:1001::/home/bob:/usr/bin/bash\n\
                      svc:x:1002:1002::/srv/svc:/usr/bin/nologin\nnobody:x:65534:65534::/:/usr/bin/nologin\n";
        assert_eq!(regular_homes(passwd, 1000), [PathBuf::from("/home/ann"), PathBuf::from("/home/bob")]);
        assert!(unfinished(&root).is_empty(), "no passwd: no users");
        create_dirs(&host(&root, "/etc")).unwrap();
        fs::write(host(&root, "/etc/passwd"), passwd).unwrap();
        assert!(!only_user(&root, Path::new("/home/ann")), "bob lives here too");
        let mark = |user: &str| {
            let dir = host(&root, &format!("/home/{user}/.local/state/bindle"));
            create_dirs(&dir).unwrap();
            fs::write(dir.join("first-login-done"), "").unwrap();
        };
        mark("ann");
        assert_eq!(unfinished(&root), [PathBuf::from("/home/bob")]);
        mark("bob");
        assert!(unfinished(&root).is_empty());
        assert!(!hooks_installed(&root));
        install(&root, false).unwrap();
        assert!(hooks_installed(&root));
        remove(&root, false).unwrap();
        assert!(!hooks_installed(&root));
        fs::write(host(&root, "/etc/passwd"), "ann:x:1000:1000::/home/ann:/usr/bin/zsh\n").unwrap();
        assert!(only_user(&root, Path::new("/home/ann")));
    }
}
