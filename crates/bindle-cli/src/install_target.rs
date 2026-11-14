//! `bindle install-target`: from the live ISO, apply the chosen profile
//! inside the freshly installed system before its first boot.
//!
//! The archinstall plugin runs it once users and the desktop exist. It runs
//! `bindle apply` in `arch-chroot` as the new user, the way it would run on
//! the installed desktop, so AUR builds happen as that user. The ISO already
//! has the network, so nothing waits for Wi-Fi at first login. sudo needs no
//! password for that user while it runs ([`grant`]: never written to the new
//! system's disk). User commands that need a running session are saved and
//! run at first login ([`pending`]).

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::apply::sys::{host, is_root, valid_name};
use crate::firstlogin;

mod grant;
pub mod pending;

pub use grant::clear_grant;
pub use pending::{PENDING, finish_pending};

/// Selection written by the plugin from the choices made before install.
pub const SELECTION: &str = "/etc/bindle/selection.toml";
/// The PATH for everything run in the new system.
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/bin";

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

/// `arch-chroot <root> runuser -u <user> -- env -i … <cmd>`: nothing of the
/// ISO's root environment (archinstall's, the plugin's) reaches the user.
fn in_target(root: &Path, acct: &Account, cmd: &[&str], extra: &[&str]) -> Command {
    let term = std::env::var("TERM").ok().filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_graphic()));
    let mut c = Command::new("arch-chroot");
    c.env_clear().env("PATH", PATH).arg(root).args(["runuser", "-u", &acct.name, "--", "env", "-i"]);
    c.arg(format!("HOME={}", acct.home))
        .arg(format!("USER={}", acct.name))
        .arg(format!("LOGNAME={}", acct.name))
        .arg(format!("PATH={PATH}"))
        .arg("LANG=C.UTF-8")
        .arg(format!("TERM={}", term.unwrap_or_else(|| "linux".into())))
        .args(extra)
        .args(cmd);
    c
}

/// `bindle <args>` as the user, told that sudo was granted for this run.
fn as_user(root: &Path, acct: &Account, args: &[&str]) -> Command {
    let marker = format!("{}=1", bindle_run::client::GRANTED_SUDO);
    let cmd: Vec<&str> = std::iter::once(firstlogin::BIN).chain(args.iter().copied()).collect();
    in_target(root, acct, &cmd, &[&marker])
}

/// What setup is at first login: finished (no wizard), or finished once the
/// saved session steps have run.
fn finished_marker(pending: Option<&str>) -> &'static str {
    match pending {
        Some(text) if text.lines().any(|l| !l.trim().is_empty()) => pending::SET_UP,
        _ => "first-login-done",
    }
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

    // Caught first: from the grant on, a signal can't end this process
    // before the grant is gone.
    let signals = grant::Signals::catch();
    let state = format!("{}/.local/state/bindle", acct.home.trim_end_matches('/'));
    let pending = format!("{state}/{PENDING}");
    let status = grant::Grant::open(root, &acct.name).and_then(|grant| {
        let mut child = as_user(root, &acct, &["apply", "--yes", "--from", SELECTION, "--defer-user", &pending])
            .spawn()
            .context("start arch-chroot")?;
        let status = signals.wait(&mut child);
        drop(grant);
        status
    });
    signals.finish();
    let status = status?;
    if !status.success() {
        bail!("Bindle's setup in the new system stopped ({status}). It opens at first login to finish.");
    }
    // Done: no wizard at first login, only the saved session steps (which
    // mark setup finished once they have run).
    let marker = finished_marker(fs::read_to_string(host(root, &pending)).ok().as_deref());
    let script = format!(r#"mkdir -p "$1" && : > "$1/{marker}""#);
    let dismissed = in_target(root, &acct, &["sh", "-c", &script, "sh", &state], &[]).status();
    if !dismissed.is_ok_and(|s| s.success()) {
        eprintln!("[bindle] couldn't mark setup as finished; Bindle may open at first login anyway");
    }
    println!("[bindle] The new system is set up.");
    Ok(())
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
    fn runs_bindle_as_the_user_in_the_chroot_with_a_clean_environment() {
        let a = Account { name: "solon".into(), home: "/home/solon".into() };
        let c = as_user(Path::new("/mnt"), &a, &["apply", "--yes"]);
        let args: Vec<String> = c.get_args().map(|s| s.to_string_lossy().into_owned()).collect();
        assert_eq!(c.get_program(), "arch-chroot");
        assert_eq!(args[..7], ["/mnt", "runuser", "-u", "solon", "--", "env", "-i"]);
        assert!(args.contains(&"HOME=/home/solon".to_string()));
        assert!(args.contains(&format!("{}=1", bindle_run::client::GRANTED_SUDO)));
        assert_eq!(args[args.len() - 3..], [firstlogin::BIN, "apply", "--yes"]);
        // Every variable is named: env -i, then NAME=value up to the program.
        let vars = &args[7..args.len() - 3];
        assert!(
            vars.iter().all(|v| v
                .split_once('=')
                .is_some_and(|(k, _)| k.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')))
        );
        // arch-chroot itself gets only PATH.
        let envs: Vec<_> = c.get_envs().filter(|(_, v)| v.is_some()).map(|(k, _)| k.to_owned()).collect();
        assert_eq!(envs, ["PATH"]);
    }

    #[test]
    fn setup_is_finished_only_once_no_steps_wait() {
        assert_eq!(finished_marker(None), "first-login-done");
        assert_eq!(finished_marker(Some("\n\n")), "first-login-done");
        assert_eq!(finished_marker(Some("{\"kind\":\"run-as-user\"}\n")), pending::SET_UP);
    }
}
