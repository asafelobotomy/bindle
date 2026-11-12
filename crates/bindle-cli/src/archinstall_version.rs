//! Which archinstall the ISO has, and how it compares with the tested one.

use std::process::Command;

pub fn archinstall_version() -> Option<String> {
    let out = Command::new("archinstall").arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit())).map(String::from)
}

fn major_minor(v: &str) -> (u32, u32) {
    let mut it = v.split('.').map(|p| p.parse().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// A note when archinstall's major.minor differs from the tested release.
pub fn version_note(found: &str, tested: &str) -> Option<String> {
    let (f, t) = (major_minor(found), major_minor(tested));
    if f == t {
        return None;
    }
    let (how, why) = if f > t {
        (
            "newer",
            "archinstall may log that the Bindle plugin \"does not support\" it, but the plugin only uses \
             the long-stable on_install hook",
        )
    } else {
        ("older", "its config schema may not accept every preset value, so check the menu before installing")
    };
    Some(format!("Note: archinstall {found} is {how} than {tested}, which Bindle was tested with; {why}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_note_on_any_major_minor_mismatch() {
        assert_eq!(version_note("4.5", "4.5"), None);
        assert_eq!(version_note("4.5.2", "4.5"), None);
        assert!(version_note("4.6", "4.5").unwrap().contains("newer than 4.5"));
        assert!(version_note("5.0", "4.5").unwrap().contains("newer"));
        assert!(version_note("4.4.1", "4.5").unwrap().contains("older than 4.5"));
        assert!(version_note("3.0", "4.5").unwrap().contains("older"));
    }
}
