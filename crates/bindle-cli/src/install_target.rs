//! `bindle install-target`: from the live ISO, apply the chosen profile
//! inside the freshly installed system before its first boot.
//!
//! The archinstall plugin runs it once users and the desktop exist. It runs
//! `bindle apply` in `arch-chroot` as the new user, the way it would run on
//! the installed desktop, so AUR builds happen as that user. The ISO already
//! has the network, so nothing waits for Wi-Fi at first login. sudo needs no
//! password for that user while it runs (a drop-in removed afterwards, also
//! on failure). User commands that need a running session are saved and run
//! at first login (`bindle first-login run`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::apply::sys::{host, is_root, valid_name, write_atomic};
use crate::firstlogin;

/// Selection written by the plugin from the choices made before install.
pub const SELECTION: &str = "/etc/bindle/selection.toml";
/// Grants the install user passwordless sudo while the apply runs.
const SUDOERS: &str = "/etc/sudoers.d/00-bindle-install";
/// User steps left for first login, under the user's state directory.
pub const PENDING: &str = "finish-at-login.jsonl";

#[derive(Debug, PartialEq)]
pub struct Account {
    pub name: String,
    pub home: String,
}

/// The first regular account in a passwd file: UID 1000–59999 with a real
/// login shell, or the one named `want`.
pub fn pick_user(passwd: &str, want: Option<&str>) -> Option<Account> {
    passwd.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        let [name, _, uid, _, _, home, shell] = f.as_slice() else { return None };
        let uid: u32 = uid.parse().ok()?;
        let regular = (1000..60000).contains(&uid) && !shell.ends_with("nologin") && !shell.ends_with("false");
        let ok = match want {
            Some(w) => *name == w,
            None => regular,
        };
        (ok && valid_name(name) && *name != "root").then(|| Account { name: name.to_string(), home: home.to_string() })
    })
}

/// Removes the sudoers drop-in however the apply ends.
struct Grant(PathBuf);

impl Drop for Grant {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.0)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("[bindle] couldn't remove {}: {e}; delete it by hand", self.0.display());
        }
    }
}

/// `arch-chroot <root> runuser -u <user> -- env … bindle <args>`.
fn as_user(root: &Path, acct: &Account, args: &[&str]) -> Command {
    let mut c = Command::new("arch-chroot");
    c.arg(root).args(["runuser", "-u", &acct.name, "--", "env"]);
    c.arg(format!("HOME={}", acct.home))
        .arg(format!("USER={}", acct.name))
        .arg(format!("LOGNAME={}", acct.name))
        .arg("PATH=/usr/local/sbin:/usr/local/bin:/usr/bin")
        .arg(format!("TERM={}", std::env::var("TERM").unwrap_or_else(|_| "linux".into())))
        .arg(firstlogin::BIN)
        .args(args);
    c
}

pub fn run(root: &Path, user: Option<&str>) -> Result<()> {
    if !is_root() {
        bail!("run as root from the live ISO");
    }
    for need in [SELECTION, firstlogin::BIN] {
        if !host(root, need).is_file() {
            bail!(
                "{} is missing in the new system; run `bindle first-login install --root {}` first",
                need,
                root.display()
            );
        }
    }
    let passwd = fs::read_to_string(host(root, "/etc/passwd")).context("read the new system's /etc/passwd")?;
    let Some(acct) = pick_user(&passwd, user) else {
        bail!("no regular user in the new system to set up for");
    };
    println!("[bindle] Setting up the new system for {} (this can take a while: AUR builds)…", acct.name);

    let sudoers = host(root, SUDOERS);
    write_atomic(&sudoers, format!("{} ALL=(ALL:ALL) NOPASSWD: ALL\n", acct.name).as_bytes(), 0o440)?;
    let grant = Grant(sudoers);

    let pending = format!("{}/.local/state/bindle/{PENDING}", acct.home.trim_end_matches('/'));
    let status = as_user(root, &acct, &["apply", "--yes", "--from", SELECTION, "--defer-user", &pending])
        .status()
        .context("start arch-chroot")?;
    drop(grant);
    if !status.success() {
        bail!("Bindle's setup in the new system stopped ({status}). It opens at first login to finish.");
    }
    // Done: no wizard at first login, only the saved session steps.
    let state = format!("{}/.local/state/bindle", acct.home.trim_end_matches('/'));
    let dismissed = Command::new("arch-chroot")
        .arg(root)
        .args(["runuser", "-u", &acct.name, "--", "sh", "-c", r#"mkdir -p "$1" && : > "$1/first-login-done""#, "sh"])
        .arg(&state)
        .status();
    if !dismissed.is_ok_and(|s| s.success()) {
        eprintln!("[bindle] couldn't mark setup as finished; Bindle may open at first login anyway");
    }
    println!("[bindle] The new system is set up.");
    Ok(())
}

/// At first login, in the user's session: run the user steps the install
/// from the ISO saved for later. If any fails, setup isn't marked finished,
/// so the usual first-login wizard opens and shows what's left.
pub fn finish_pending() {
    for dir in firstlogin::state_dirs() {
        // Claimed by renaming: autostart and the user unit can both fire.
        let claimed = dir.join(format!("{PENDING}.running"));
        if fs::rename(dir.join(PENDING), &claimed).is_err() {
            continue;
        }
        let text = fs::read_to_string(&claimed).unwrap_or_default();
        let _ = fs::remove_file(&claimed);
        let mut failed = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let Ok(bindle_core::Step::RunAsUser { argv, manual, .. }) = serde_json::from_str(line) else { continue };
            let Some((prog, args)) = argv.split_first() else { continue };
            let ok = Command::new(prog)
                .args(args)
                .status()
                .ok()
                .and_then(|s| s.code())
                .is_some_and(|c| c == 0 || manual.contains_key(&c.to_string()));
            if !ok {
                failed.push(argv.join(" "));
            }
        }
        if !failed.is_empty() {
            eprintln!("[bindle] setup steps that didn't finish: {}", failed.join("; "));
            for d in firstlogin::state_dirs() {
                let _ = fs::remove_file(d.join("first-login-done"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str = "root:x:0:0::/root:/usr/bin/bash\n\
        nobody:x:65534:65534:Kernel Overflow User:/:/usr/bin/nologin\n\
        dbus:x:81:81:System Message Bus:/:/usr/bin/nologin\n\
        svc:x:1001:1001::/srv/svc:/usr/bin/nologin\n\
        solon:x:1000:1000::/home/solon:/bin/bash\n\
        guest:x:1002:1002::/home/guest:/bin/zsh\n";

    #[test]
    fn first_regular_user_with_a_shell() {
        let a = pick_user(PASSWD, None).unwrap();
        assert_eq!(a, Account { name: "solon".into(), home: "/home/solon".into() });
        assert_eq!(pick_user(PASSWD, Some("guest")).unwrap().home, "/home/guest");
        assert!(pick_user(PASSWD, Some("root")).is_none());
        assert!(pick_user(PASSWD, Some("missing")).is_none());
        assert!(pick_user("root:x:0:0::/root:/bin/bash\n", None).is_none());
    }

    #[test]
    fn runs_bindle_as_the_user_in_the_chroot() {
        let a = Account { name: "solon".into(), home: "/home/solon".into() };
        let c = as_user(Path::new("/mnt"), &a, &["apply", "--yes"]);
        let args: Vec<String> = c.get_args().map(|s| s.to_string_lossy().into_owned()).collect();
        assert_eq!(c.get_program(), "arch-chroot");
        assert_eq!(args[..6], ["/mnt", "runuser", "-u", "solon", "--", "env"]);
        assert!(args.contains(&"HOME=/home/solon".to_string()));
        assert_eq!(args[args.len() - 3..], [firstlogin::BIN, "apply", "--yes"]);
    }
}
