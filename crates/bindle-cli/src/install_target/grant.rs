//! The install user's passwordless sudo, for the apply in the new system
//! only. The rule never lands on the new system's disk: it lives in a
//! root-only file under the ISO's /run (memory), bind-mounted over an empty
//! placeholder in the target's /etc/sudoers.d for as long as arch-chroot
//! runs. A crash leaves at most the empty placeholder (harmless to sudo),
//! which the next run, the archinstall plugin, `bindle install` and the new
//! system's tmpfiles rule all remove; a restart drops the mount.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::apply::sys::{host, write_atomic_system};

/// Where sudo in the new system finds the rule. sudo reads the drop-ins in
/// name order and the last match wins, so this must sort after
/// archinstall's `00_<user>` (which asks for a password).
pub const SUDOERS: &str = "/etc/sudoers.d/zz-bindle-install";
/// On the live ISO (tmpfs): the rule itself, root-only.
const RULE_DIR: &str = "/run/bindle-install";

/// A login name sudo and Bindle can both take literally.
pub fn valid_user(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// The sudoers line: this user, every command, no password.
pub fn rule(user: &str) -> Result<String> {
    if !valid_user(user) || user == "root" {
        bail!("refusing to grant sudo to `{user}`: not a plain login name");
    }
    Ok(format!(
        "# Managed by Bindle: only while it sets up the new system from the live ISO.\n\
                {user} ALL=(ALL:ALL) NOPASSWD: ALL\n"
    ))
}

/// Unescape a mount point from /proc/self/mountinfo (`\040` for a space…).
fn unescape(field: &str) -> String {
    let b = field.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let oct = b.get(i + 1..i + 4).and_then(|o| u8::from_str_radix(std::str::from_utf8(o).ok()?, 8).ok());
        match (b[i], oct) {
            (b'\\', Some(byte)) => {
                out.push(byte);
                i += 4;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Something is mounted on `path` (according to `mountinfo`'s text).
fn mounted_in(mountinfo: &str, path: &Path) -> bool {
    let want = path.to_string_lossy();
    mountinfo.lines().any(|l| l.split(' ').nth(4).is_some_and(|m| unescape(m) == want))
}

fn is_mounted(path: &Path) -> bool {
    mounted_in(&fs::read_to_string("/proc/self/mountinfo").unwrap_or_default(), path)
}

fn umount(path: &Path) {
    let quiet = |args: &[&str]| {
        Command::new("umount").args(args).arg(path).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
    };
    // Lazily if something holds it open: it must not stay reachable.
    if !quiet(&[]) && !quiet(&["-l"]) {
        eprintln!("[bindle] couldn't unmount {}", path.display());
    }
}

/// Remove any grant a run left behind in the new system at `root` (and the
/// ISO's copy of the rule). Safe to call any number of times.
pub fn clear_grant(root: &Path) {
    let placeholder = host(root, SUDOERS);
    while is_mounted(&placeholder) {
        umount(&placeholder);
        if is_mounted(&placeholder) {
            break;
        }
    }
    match fs::remove_file(&placeholder) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => eprintln!("[bindle] couldn't remove {}: {e}; delete it by hand", placeholder.display()),
    }
    let _ = fs::remove_dir_all(RULE_DIR);
}

/// Live while the apply runs; dropping it takes the grant away.
pub struct Grant {
    root: PathBuf,
}

impl Grant {
    pub fn open(root: &Path, user: &str) -> Result<Grant> {
        let text = rule(user)?;
        clear_grant(root);
        fs::DirBuilder::new().mode(0o700).create(RULE_DIR).with_context(|| format!("create {RULE_DIR}"))?;
        // From here on, dropping `grant` undoes whatever was done.
        let grant = Grant { root: root.to_path_buf() };
        let rule_file = Path::new(RULE_DIR).join("rule");
        write_atomic_system(&rule_file, text.as_bytes(), 0o440)?;
        let placeholder = host(root, SUDOERS);
        write_atomic_system(&placeholder, b"", 0o440)?;
        let bound = Command::new("mount").arg("--bind").arg(&rule_file).arg(&placeholder).status();
        if !bound.is_ok_and(|s| s.success()) {
            bail!("couldn't bind-mount the sudo rule into the new system");
        }
        if host(root, "/usr/bin/visudo").is_file() {
            let checked = Command::new("arch-chroot")
                .arg(root)
                .args(["visudo", "-cf", SUDOERS])
                .env_clear()
                .env("PATH", "/usr/local/sbin:/usr/local/bin:/usr/bin")
                .stdout(Stdio::null())
                .status();
            if !checked.is_ok_and(|s| s.success()) {
                bail!("visudo rejected the sudo rule for `{user}`");
            }
        }
        Ok(grant)
    }
}

impl Drop for Grant {
    fn drop(&mut self) {
        clear_grant(&self.root);
    }
}

/// The last of SIGINT, SIGTERM or SIGHUP that arrived (0: none).
static SIGNALLED: AtomicI32 = AtomicI32::new(0);
const CAUGHT: [libc::c_int; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

extern "C" fn note(sig: libc::c_int) {
    SIGNALLED.store(sig, Ordering::SeqCst);
}

/// While held, SIGINT, SIGTERM and SIGHUP don't end this process before the
/// grant is gone: they're passed on to the apply, and `finish` ends the
/// process the same way once it has cleaned up.
pub struct Signals {
    old: Vec<(libc::c_int, libc::sigaction)>,
}

impl Signals {
    pub fn catch() -> Signals {
        SIGNALLED.store(0, Ordering::SeqCst);
        let mut old = Vec::new();
        for sig in CAUGHT {
            // SAFETY: sigaction with zeroed (valid) structs we own; the handler
            // only stores to an atomic, which is async-signal-safe.
            unsafe {
                let mut act: libc::sigaction = std::mem::zeroed();
                act.sa_sigaction = note as extern "C" fn(libc::c_int) as libc::sighandler_t;
                act.sa_flags = libc::SA_RESTART;
                let mut prev: libc::sigaction = std::mem::zeroed();
                if libc::sigaction(sig, &act, &mut prev) == 0 {
                    old.push((sig, prev));
                }
            }
        }
        Signals { old }
    }

    /// Wait for `child`, passing on any of those signals that arrive.
    pub fn wait(&self, child: &mut Child) -> Result<ExitStatus> {
        let mut passed = 0;
        loop {
            if let Some(status) = child.try_wait().context("wait for arch-chroot")? {
                return Ok(status);
            }
            let sig = SIGNALLED.load(Ordering::SeqCst);
            if sig != 0 && sig != passed {
                // SAFETY: kill(2) on our own child's pid; no memory is passed.
                unsafe { libc::kill(child.id() as libc::pid_t, sig) };
                passed = sig;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Put the old handlers back; if a signal came, end the way it would have.
    pub fn finish(self) {
        drop(self);
        let sig = SIGNALLED.swap(0, Ordering::SeqCst);
        if sig != 0 {
            eprintln!("[bindle] stopped; the temporary sudo rule is removed");
            // SAFETY: raise(2) with a valid signal number, default disposition.
            unsafe { libc::raise(sig) };
        }
    }
}

impl Drop for Signals {
    fn drop(&mut self) {
        for (sig, prev) in self.old.drain(..) {
            // SAFETY: restoring the disposition sigaction gave us earlier.
            unsafe { libc::sigaction(sig, &prev, std::ptr::null_mut()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_login_names_get_a_rule() {
        let r = rule("solon").unwrap();
        assert!(r.ends_with("\nsolon ALL=(ALL:ALL) NOPASSWD: ALL\n"), "{r}");
        assert!(r.lines().all(|l| l.starts_with('#') || l.starts_with("solon ")));
        for ok in ["a", "_x", "user-1", "u_2"] {
            assert!(valid_user(ok), "{ok}");
        }
        for bad in ["", "root", "Solon", "1user", "-x", "a b", "a,b", "a\nALL", "a=b", "ä", &"a".repeat(33)] {
            assert!(rule(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn grant_sorts_after_archinstalls_sudoers() {
        // archinstall writes `00_<user>`; sudo's last matching rule wins.
        let ours = SUDOERS.rsplit('/').next().unwrap();
        let mut names = [ours, "00_solon", "00_zz", "_tester", "wheel"];
        names.sort_unstable();
        assert_eq!(names.last(), Some(&ours));
    }

    #[test]
    fn reads_mount_points_from_mountinfo() {
        let info = "36 35 98:0 /mnt1 /mnt/etc/sudoers.d/zz-bindle-install rw,noatime master:1 - ext3 /dev/root rw\n\
                    37 35 98:0 / /mnt/with\\040space rw - ext4 /dev/sda1 rw\n";
        assert!(mounted_in(info, Path::new("/mnt/etc/sudoers.d/zz-bindle-install")));
        assert!(mounted_in(info, Path::new("/mnt/with space")));
        assert!(!mounted_in(info, Path::new("/mnt/etc")));
    }

    #[test]
    fn clearing_removes_a_leftover_and_is_idempotent() {
        let root = crate::apply::sys::test_dir("install-grant");
        let leftover = host(&root, SUDOERS);
        fs::create_dir_all(leftover.parent().unwrap()).unwrap();
        // Also what an older Bindle left: the rule itself, on the disk.
        fs::write(&leftover, "solon ALL=(ALL:ALL) NOPASSWD: ALL\n").unwrap();
        fs::write(leftover.with_file_name("00_solon"), "solon ALL=(ALL) ALL\n").unwrap();
        clear_grant(&root);
        assert!(!leftover.exists());
        assert!(leftover.with_file_name("00_solon").exists(), "only Bindle's file goes");
        clear_grant(&root);
        assert!(!is_mounted(&leftover));
    }
}
