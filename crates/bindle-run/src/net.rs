//! Is there an internet connection? Checked before an apply changes anything,
//! so a missing connection is a plain message instead of pacman's mirror errors.

use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);
/// Public resolvers by address (Cloudflare, Quad9): reachable without DNS.
const BY_ADDRESS: [&str; 2] = ["1.1.1.1:443", "9.9.9.9:443"];

/// Why the connection isn't usable, as `(what, what to do)`.
pub fn problem() -> Option<(&'static str, &'static str)> {
    let resolved = ("archlinux.org", 443).to_socket_addrs().ok().and_then(|mut a| a.next());
    let reachable = |addr: &SocketAddr| TcpStream::connect_timeout(addr, TIMEOUT).is_ok();
    let by_address = || BY_ADDRESS.iter().filter_map(|a| a.parse().ok()).any(|a: SocketAddr| reachable(&a));
    diagnose(resolved.map(|a| reachable(&a)), by_address)
}

/// `resolved`: None when the name didn't resolve, else whether it connected.
fn diagnose(resolved: Option<bool>, by_address: impl Fn() -> bool) -> Option<(&'static str, &'static str)> {
    match resolved {
        Some(true) => None,
        Some(false) | None if !by_address() => Some((
            "This computer isn't connected to the internet.",
            "Connect first. Wi-Fi: `nmcli device wifi connect \"<network>\" --ask` (`nmcli device wifi list` shows networks).",
        )),
        None => Some((
            "This computer is online but can't look up names (DNS).",
            "Try `sudo systemctl restart NetworkManager`, and check /etc/resolv.conf has a nameserver.",
        )),
        Some(false) => Some((
            "archlinux.org can't be reached right now, though the internet works.",
            "Wait a few minutes; if it persists, a firewall or proxy may be blocking it.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_no_connection_from_no_dns() {
        assert_eq!(diagnose(Some(true), || panic!("not checked")), None);
        assert!(diagnose(None, || false).unwrap().0.contains("isn't connected"));
        assert!(diagnose(None, || true).unwrap().0.contains("DNS"));
        assert!(diagnose(Some(false), || true).unwrap().0.contains("archlinux.org"));
        assert!(diagnose(Some(false), || false).unwrap().0.contains("isn't connected"));
    }
}
