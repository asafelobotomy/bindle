//! Start a polkit agent for the desktop window when the session has none.
//!
//! archinstall's Hyprland (and other bare compositors) install an agent but
//! never start one, so on the first login pkexec had no one to ask for the
//! password and the window fell back to a terminal. An agent started while one
//! is already registered fails to register and exits, so trying is harmless on
//! desktops that run their own.

use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

/// Installed agents, in order of preference (Qt first: Bindle's window is Qt).
pub const AGENTS: &[&str] = &[
    "/usr/lib/polkit-kde-authentication-agent-1",
    "/usr/lib/hyprpolkitagent/hyprpolkitagent",
    "/usr/bin/lxqt-policykit-agent",
    "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1",
    "/usr/lib/mate-polkit/polkit-mate-authentication-agent-1",
];

/// The first agent in `candidates` that `exists` accepts.
pub fn pick<'a>(candidates: &[&'a str], exists: impl Fn(&str) -> bool) -> Option<&'a str> {
    candidates.iter().copied().find(|p| exists(p))
}

/// Start the first installed agent, detached, and give it a moment to
/// register. It outlives Bindle and keeps serving the session.
pub fn ensure() {
    let Some(agent) = pick(AGENTS, |p| Path::new(p).is_file()) else { return };
    let started = Command::new(agent).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    if started.is_ok() {
        thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_first_installed_agent() {
        let installed = ["/usr/lib/hyprpolkitagent/hyprpolkitagent", "/usr/bin/lxqt-policykit-agent"];
        assert_eq!(pick(AGENTS, |p| installed.contains(&p)), Some(installed[0]));
        assert_eq!(pick(AGENTS, |p| p == AGENTS[0]), Some(AGENTS[0]));
        assert_eq!(pick(AGENTS, |_| false), None);
    }
}
