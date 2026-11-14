//! Open Bindle on each user's first login after install.
//!
//! `bindle first-login install` (root, e.g. from archinstall's custom commands
//! or with `--root /mnt` from the live ISO) installs the binary system-wide plus
//! an XDG autostart entry (desktops), a systemd user service that waits for any
//! graphical session (window managers without XDG autostart: Sway, niri, i3…),
//! and a profile.d hook (console logins). Each user gets the wizard once; a
//! per-user marker records that, and a short-lived lock stops double launches.

use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::apply::sys::{create_dirs, host, is_root, shell_words, write_atomic_system};

mod users;
pub use users::{only_user, unfinished};

pub const BIN: &str = "/usr/local/bin/bindle";
/// The desktop window, installed when it ships next to `bindle`.
pub const GUI_BIN: &str = "/usr/local/bin/bindle-gui";
pub const AUTOSTART: &str = "/etc/xdg/autostart/bindle-first-login.desktop";
pub const PROFILE_HOOK: &str = "/etc/profile.d/bindle-first-login.sh";
pub const LAUNCHER: &str = "/usr/local/share/applications/bindle.desktop";
pub const USER_UNIT: &str = "/etc/systemd/user/bindle-first-login.service";
pub const USER_UNIT_WANTS: &str = "/etc/systemd/user/default.target.wants/bindle-first-login.service";
/// The desktop window's password dialog message (pkexec).
pub const POLKIT: &str = "/usr/share/polkit-1/actions/io.github.asafelobotomy.bindle.policy";
const POLICY: &str = include_str!("../../../packaging/polkit/io.github.asafelobotomy.bindle.policy");
/// At every start, remove the live-ISO install's sudo rule should one survive.
pub const TMPFILES: &str = "/etc/tmpfiles.d/bindle.conf";
const TMPFILES_RULE: &str = "# Managed by Bindle: the live-ISO install's passwordless sudo never outlives it.\n\
                             r /etc/sudoers.d/zz-bindle-install\n";
/// uQ Settings' "Re-run setup" page (read only from /usr/share/uq), and the
/// path its commands run Bindle by (they may only run programs under /usr/lib
/// or /usr/bin). Harmless without uQ; uQ installed later picks the page up.
pub const UQ_PAGE_DIR: &str = "/usr/share/uq/settings/pages/bindle";
pub const UQ_PAGE: &str = "/usr/share/uq/settings/pages/bindle/page.toml";
pub const UQ_PAGE_QML: &str = "/usr/share/uq/settings/pages/bindle/Page.qml";
pub const UQ_PAGE_BIN: &str = "/usr/lib/bindle/bindle";
const PAGE_TOML: &str = include_str!("../../../uq/pages/bindle/page.toml");
const PAGE_QML: &str = include_str!("../../../uq/pages/bindle/Page.qml");

const USER_UNIT_FILE: &str = "[Unit]
Description=Open Bindle on the first graphical login
# Never for greeters (gdm, sddm, lightdm…) or other system accounts.
ConditionUser=!@system
# Bindle always writes this marker (also when XDG_STATE_HOME points elsewhere).
ConditionPathExists=!%h/.local/state/bindle/first-login-done

[Service]
ExecStart=/usr/local/bin/bindle first-login watch
# The watcher exits once the terminal is open; don't take the terminal with it.
KillMode=process

[Install]
WantedBy=default.target
";

const AUTOSTART_ENTRY: &str = "[Desktop Entry]
Type=Application
Name=Bindle
Comment=Finish setting up your new system
Exec=/usr/local/bin/bindle first-login run
Icon=bindle
Terminal=false
NoDisplay=true
X-GNOME-Autostart-Delay=3
";

const LAUNCHER_ENTRY: &str = "[Desktop Entry]
Type=Application
Name=Bindle
Comment=Set up and optimise this system
Exec=/usr/local/bin/bindle open
Icon=bindle
Terminal=false
Categories=System;Settings;
";

const HOOK: &str = r#"# Managed by Bindle: open the setup wizard once, on a user's first console login.
# Only interactive shells: display managers (greetd, ly…) start sessions through
# `sh -l -c`, which also reads /etc/profile on the VT before the desktop starts.
case $- in *i*) ;; *) return 0 2>/dev/null || exit 0 ;; esac
# Regular users only (not root or system accounts), in a `user` session.
_bindle_uid_min=$(awk '$1 == "UID_MIN" { print $2 }' /etc/login.defs 2>/dev/null)
if [ -t 0 ] && [ -t 1 ] && [ -z "${DISPLAY}${WAYLAND_DISPLAY}${SSH_CONNECTION}" ] \
    && [ "$(id -u)" -ge "${_bindle_uid_min:-1000}" ] 2>/dev/null \
    && [ "${XDG_SESSION_CLASS:-user}" = user ] && [ -x /usr/local/bin/bindle ] \
    && [ ! -e "${XDG_STATE_HOME:-$HOME/.local/state}/bindle/first-login-done" ] \
    && [ ! -e "$HOME/.local/state/bindle/first-login-done" ]; then
    /usr/local/bin/bindle first-login run
fi
unset _bindle_uid_min
"#;

fn require_root(root: &Path) -> Result<()> {
    if root == Path::new("/") && !is_root() {
        bail!("needs root: sudo bindle first-login …");
    }
    Ok(())
}

pub fn install(root: &Path, tty_hook: bool) -> Result<Vec<String>> {
    require_root(root)?;
    let mut done = Vec::new();
    let exe = env::current_exe().context("locate own executable")?;
    let target = host(root, BIN);
    let same = fs::canonicalize(&exe).ok() == fs::canonicalize(&target).ok();
    if !same {
        let data = fs::read(&exe).context("read own executable")?;
        write_atomic_system(&target, &data, 0o755)?;
        done.push(format!("installed {BIN}"));
    }
    let gui = exe.with_file_name("bindle-gui");
    let gui_target = host(root, GUI_BIN);
    if gui.is_file() && fs::canonicalize(&gui).ok() != fs::canonicalize(&gui_target).ok() {
        let data = fs::read(&gui).context("read bindle-gui")?;
        write_atomic_system(&gui_target, &data, 0o755)?;
        done.push(format!("installed {GUI_BIN}"));
    }
    let mut files = vec![
        (AUTOSTART, AUTOSTART_ENTRY),
        (LAUNCHER, LAUNCHER_ENTRY),
        (USER_UNIT, USER_UNIT_FILE),
        (POLKIT, POLICY),
        (UQ_PAGE, PAGE_TOML),
        (UQ_PAGE_QML, PAGE_QML),
        (TMPFILES, TMPFILES_RULE),
    ];
    if tty_hook {
        files.push((PROFILE_HOOK, HOOK));
    }
    for (path, content) in files {
        write_atomic_system(&host(root, path), content.as_bytes(), 0o644)?;
        done.push(format!("wrote {path}"));
    }
    let icons = crate::icons::files();
    for (path, data) in &icons {
        write_atomic_system(&host(root, path), data, 0o644)?;
    }
    done.push(format!("wrote the icon ({} sizes) under /usr/local/share/icons/hicolor", icons.len()));
    // What `systemctl --global enable` does, without needing systemd running (chroot).
    let wants = host(root, USER_UNIT_WANTS);
    create_dirs(wants.parent().context("wants dir")?)?;
    let _ = fs::remove_file(&wants);
    symlink("../bindle-first-login.service", &wants).context("enable user unit")?;
    done.push(format!("enabled {USER_UNIT} for all users"));
    let link = host(root, UQ_PAGE_BIN);
    create_dirs(link.parent().context("page bin dir")?)?;
    let _ = fs::remove_file(&link);
    symlink(BIN, &link).with_context(|| format!("link {UQ_PAGE_BIN}"))?;
    done.push(format!("linked {UQ_PAGE_BIN} to {BIN} for uQ Settings' Re-run setup page"));
    done.push(crate::bindo_cmd::link_names(root)?);
    Ok(done)
}

pub fn remove(root: &Path, purge: bool) -> Result<Vec<String>> {
    require_root(root)?;
    let icons: Vec<String> = crate::icons::files().into_iter().map(|(p, _)| p).collect();
    let mut paths = vec![AUTOSTART, PROFILE_HOOK, USER_UNIT_WANTS, USER_UNIT];
    if purge {
        paths.extend([LAUNCHER, POLKIT, TMPFILES, UQ_PAGE, UQ_PAGE_QML, UQ_PAGE_BIN, GUI_BIN, BIN]);
        paths.extend(crate::bindo_cmd::NAMES);
        paths.extend(icons.iter().map(String::as_str));
    }
    let mut done = Vec::new();
    for p in paths {
        let h = host(root, p);
        if h.symlink_metadata().is_ok() {
            fs::remove_file(&h).with_context(|| format!("remove {p}"))?;
            done.push(format!("removed {p}"));
        }
    }
    if purge {
        // Only once empty: never anything someone else put there.
        for dir in [UQ_PAGE_DIR, "/usr/lib/bindle"] {
            let _ = fs::remove_dir(host(root, dir));
        }
    }
    Ok(done)
}

pub fn status(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = [
        BIN,
        GUI_BIN,
        AUTOSTART,
        USER_UNIT,
        USER_UNIT_WANTS,
        PROFILE_HOOK,
        LAUNCHER,
        crate::icons::SVG_PATH,
        POLKIT,
        UQ_PAGE,
        UQ_PAGE_BIN,
    ]
    .iter()
    .map(|p| format!("{} {p}", if host(root, p).exists() { "✓" } else { "·" }))
    .collect();
    if marker_path().is_some() {
        let state = if is_done() {
            "done — won't open again"
        } else if is_seen() {
            "not finished — reminder notification at next login"
        } else {
            "pending — opens at next login"
        };
        out.push(format!("this user: {state}"));
    }
    out
}

/// Where the "done" marker lives: `$XDG_STATE_HOME` and the default
/// `~/.local/state`. Both are written, so the systemd unit (which can only
/// check `%h/.local/state`) and shells with a different `XDG_STATE_HOME`
/// agree on whether setup is finished.
pub fn state_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = env::var_os("XDG_STATE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .into_iter()
        .chain(env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map(|s| s.join("bindle"))
        .collect();
    dirs.dedup();
    dirs
}

pub fn marker_path() -> Option<PathBuf> {
    state_dirs().into_iter().next().map(|d| d.join("first-login-done"))
}

fn exists(name: &str) -> bool {
    state_dirs().iter().any(|d| d.join(name).exists())
}

fn mark(name: &str) -> Result<()> {
    let dirs = state_dirs();
    if dirs.is_empty() {
        bail!("no home directory");
    }
    for dir in dirs {
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(name), "")?;
    }
    Ok(())
}

/// The wizard has opened at a login before; later logins get a reminder.
pub fn is_seen() -> bool {
    exists("first-login-seen")
}

pub fn mark_seen() -> Result<()> {
    mark("first-login-seen")
}

pub fn is_done() -> bool {
    exists("first-login-done")
}

pub fn mark_done() -> Result<()> {
    mark("first-login-done")
}

/// The login hooks (autostart, user unit, console hook) are still installed.
pub fn hooks_installed(root: &Path) -> bool {
    [AUTOSTART, USER_UNIT_WANTS, PROFILE_HOOK].iter().any(|p| host(root, p).symlink_metadata().is_ok())
}

/// JSON to merge into an archinstall configuration file. The download goes
/// to a temporary file (https only); with `sha256` it is verified there, and
/// only then made executable and moved to `BIN`.
pub fn archinstall_snippet(url: &str, sha256: Option<&str>) -> Result<String> {
    let tmp = format!("{BIN}.download");
    let mut cmd = format!("curl -fsSL --proto '=https' --tlsv1.2 -o {tmp} {}", shell_words(&[url.to_string()]));
    if let Some(sum) = sha256 {
        if sum.len() != 64 || !sum.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("--sha256 needs a 64-character hex SHA-256 digest");
        }
        cmd.push_str(&format!(" && echo '{}  {tmp}' | sha256sum -c --quiet -", sum.to_ascii_lowercase()));
    }
    cmd.push_str(&format!(" && chmod 755 {tmp} && mv -f {tmp} {BIN} && {BIN} first-login install"));
    let cmd = format!("{{ {cmd}; }} || {{ rm -f {tmp}; false; }}");
    Ok(serde_json::to_string_pretty(&serde_json::json!({ "custom_commands": [cmd] }))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_and_remove_into_a_root() {
        let root = crate::apply::sys::test_dir("firstlogin");
        let done = install(&root, true).unwrap();
        assert!(done.iter().any(|d| d.contains(BIN)));
        assert!(host(&root, AUTOSTART).exists() && host(&root, PROFILE_HOOK).exists());
        let entry = fs::read_to_string(host(&root, AUTOSTART)).unwrap();
        assert!(entry.contains("Exec=/usr/local/bin/bindle first-login run"));
        assert_eq!(
            fs::read_link(host(&root, USER_UNIT_WANTS)).unwrap(),
            PathBuf::from("../bindle-first-login.service")
        );
        let unit = fs::read_to_string(host(&root, USER_UNIT)).unwrap();
        assert!(unit.contains("ConditionUser=!@system"));
        assert!(fs::read_to_string(host(&root, PROFILE_HOOK)).unwrap().contains("UID_MIN"));
        let policy = fs::read_to_string(host(&root, POLKIT)).unwrap();
        // The helper's own path and first argument (client.rs: pkexec <bindle> helper), one approval each.
        assert!(policy.contains(&format!(">{BIN}<")) && policy.contains(">helper<"));
        assert!(policy.contains("<allow_active>auth_admin</allow_active>"));
        let tmpfiles = fs::read_to_string(host(&root, TMPFILES)).unwrap();
        assert!(tmpfiles.lines().any(|l| l == "r /etc/sudoers.d/zz-bindle-install"), "{tmpfiles}");
        // uQ Settings' page runs Bindle by a path under /usr/lib.
        assert!(fs::read_to_string(host(&root, UQ_PAGE)).unwrap().contains(&format!("\"{UQ_PAGE_BIN}\"")));
        assert!(host(&root, UQ_PAGE_QML).is_file());
        assert_eq!(fs::read_link(host(&root, UQ_PAGE_BIN)).unwrap(), PathBuf::from(BIN));
        remove(&root, false).unwrap();
        assert!(!host(&root, AUTOSTART).exists() && host(&root, BIN).exists());
        assert!(host(&root, USER_UNIT_WANTS).symlink_metadata().is_err());
        remove(&root, true).unwrap();
        assert!(!host(&root, BIN).exists() && !host(&root, POLKIT).exists());
        assert!(crate::bindo_cmd::NAMES.iter().all(|n| host(&root, n).symlink_metadata().is_err()));
        assert!(!host(&root, UQ_PAGE_DIR).exists() && host(&root, UQ_PAGE_BIN).symlink_metadata().is_err());
    }

    /// The page only runs Bindle commands that exist, through the linked path.
    #[test]
    fn the_uq_page_runs_real_subcommands() {
        let page: toml::Value = toml::from_str(PAGE_TOML).unwrap();
        assert_eq!(page["id"].as_str(), Some("bindle"));
        let commands = page["command"].as_array().unwrap();
        let argv = |id: &str| -> Vec<String> {
            let c = commands.iter().find(|c| c["id"].as_str() == Some(id)).unwrap();
            c["argv"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect()
        };
        assert_eq!(argv("status"), [UQ_PAGE_BIN, "status", "--json"]);
        assert_eq!(argv("open"), [UQ_PAGE_BIN, "open", "--detach"]);
        use clap::CommandFactory;
        let cli = crate::Cli::command();
        for id in ["status", "open"] {
            let args = argv(id);
            cli.clone().try_get_matches_from(&args).unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        // Only UqLabel shows text (uQ refuses Text and Label).
        for refused in ["Text {", "Label {", "TextEdit", "TextInput", "createQmlObject"] {
            let plain = PAGE_QML.replace("UqLabel {", "");
            assert!(!plain.contains(refused), "Page.qml uses {refused}");
        }
    }

    #[test]
    fn snippet_quotes_the_url_and_checks_the_digest() {
        let plain = archinstall_snippet("https://example.invalid/bindle", None).unwrap();
        assert!(plain.contains("first-login install") && !plain.contains("sha256sum"));
        let evil = archinstall_snippet("https://x/'; rm -rf / #", None).unwrap();
        let cmd: serde_json::Value = serde_json::from_str(&evil).unwrap();
        assert!(cmd["custom_commands"][0].as_str().unwrap().contains(r"'https://x/'\''; rm -rf / #'"));
        assert!(plain.contains("--proto '=https' --tlsv1.2"));
        let sum = "A".repeat(64);
        let checked = archinstall_snippet("https://x/b", Some(&sum)).unwrap();
        let cmd: serde_json::Value = serde_json::from_str(&checked).unwrap();
        let cmd = cmd["custom_commands"][0].as_str().unwrap();
        // Checked as a temporary file; only then executable and in place.
        let tmp = format!("{BIN}.download");
        let check = cmd.find(&format!("{}  {tmp}' | sha256sum -c", "a".repeat(64))).unwrap();
        assert!(cmd.find(&format!("-o {tmp} ")).unwrap() < check);
        assert!(check < cmd.find(&format!("chmod 755 {tmp}")).unwrap());
        assert!(check < cmd.find(&format!("mv -f {tmp} {BIN}")).unwrap());
        assert!(!cmd.contains(&format!("-o {BIN} ")) && cmd.ends_with(&format!("|| {{ rm -f {tmp}; false; }}")));
        assert!(archinstall_snippet("https://x/b", Some("abc")).is_err());
    }
}
