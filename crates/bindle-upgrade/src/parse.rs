//! Turn command output and logs into data. Pure; each parser is tested on
//! real output.

use serde::{Deserialize, Serialize};

/// One package with a newer version available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Update {
    pub name: String,
    pub old: String,
    pub new: String,
}

/// `name old -> new` lines, as `checkupdates`, `paru -Qua` and `yay -Qua`
/// print them (paru may append ` [ignored]`, which are left out).
pub fn updates(text: &str) -> Vec<Update> {
    text.lines()
        .filter(|l| !l.contains("[ignored]"))
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let (name, old, arrow, new) = (w.next()?, w.next()?, w.next()?, w.next()?);
            (arrow == "->").then(|| Update { name: name.into(), old: old.into(), new: new.into() })
        })
        .collect()
}

/// Application ids from `flatpak remote-ls --updates --columns=application`
/// (a header line, when there is one, is not an id).
pub fn flatpak_apps(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.contains('.') && !l.contains(char::is_whitespace))
        .map(String::from)
        .collect()
}

/// `Device → version` from `fwupdmgr get-updates --json`.
pub fn firmware(json: &str) -> Result<Vec<String>, String> {
    #[derive(Deserialize)]
    struct Out {
        #[serde(rename = "Devices", default)]
        devices: Vec<Device>,
    }
    #[derive(Deserialize)]
    struct Device {
        #[serde(rename = "Name", default)]
        name: String,
        #[serde(rename = "Releases", default)]
        releases: Vec<Release>,
    }
    #[derive(Deserialize)]
    struct Release {
        #[serde(rename = "Version", default)]
        version: String,
    }
    let out: Out = serde_json::from_str(json).map_err(|e| format!("unexpected fwupdmgr output: {e}"))?;
    Ok(out
        .devices
        .into_iter()
        .filter_map(|d| d.releases.first().map(|r| format!("{} → {}", d.name, r.version)))
        .collect())
}

/// Days since 1970-01-01 of a civil date (proleptic Gregorian).
pub fn day(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A day number as `YYYY-MM-DD`.
pub fn ymd(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{m:02}-{d:02}", yoe + era * 400 + i64::from(m <= 2))
}

/// `YYYY-MM-DD` at the start of `s`.
pub fn iso_day(s: &str) -> Option<i64> {
    let (y, rest) = (s.get(0..4)?, s.get(4..)?);
    let (m, d) = (rest.get(1..3)?, rest.get(4..6)?);
    if rest.as_bytes().first() != Some(&b'-') || rest.as_bytes().get(3) != Some(&b'-') {
        return None;
    }
    let (m, d): (u32, u32) = (m.parse().ok()?, d.parse().ok()?);
    let y: i64 = y.parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| day(y, m, d))
}

/// The day of the last full system upgrade in pacman.log.
pub fn last_upgrade(log: &str) -> Option<i64> {
    log.lines().rev().find(|l| l.contains("starting full system upgrade")).and_then(|l| iso_day(l.strip_prefix('[')?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkupdates_and_aur_lines() {
        let text = "linux 6.17.1.arch1-1 -> 6.17.2.arch1-1\nmesa 1:25.2.4-1 -> 1:25.2.5-1\n\
                    paru-bin 2.1.0-1 -> 2.2.0-1 [ignored]\n:: Checking AUR\n";
        let u = updates(text);
        assert_eq!(u.len(), 2);
        assert_eq!(u[0], Update { name: "linux".into(), old: "6.17.1.arch1-1".into(), new: "6.17.2.arch1-1".into() });
        assert_eq!(u[1].new, "1:25.2.5-1");
    }

    #[test]
    fn flatpak_ids_skip_headers() {
        assert_eq!(
            flatpak_apps("Application ID\norg.mozilla.firefox\norg.gnome.Platform\n\n"),
            ["org.mozilla.firefox", "org.gnome.Platform"]
        );
    }

    #[test]
    fn fwupd_json() {
        let json = r#"{"Devices":[{"Name":"System Firmware","Releases":[{"Version":"1.2.3"}]},{"Name":"SSD"}]}"#;
        assert_eq!(firmware(json).unwrap(), ["System Firmware → 1.2.3"]);
        assert!(firmware("No updates available").is_err());
    }

    #[test]
    fn days_count_from_the_epoch() {
        assert_eq!(day(1970, 1, 1), 0);
        assert_eq!(day(2000, 3, 1), 11_017);
        assert_eq!(day(2026, 11, 13) - day(2026, 10, 14), 30);
        assert_eq!(iso_day("2026-11-13T10:00:00+0000"), Some(day(2026, 11, 13)));
        assert_eq!(iso_day("2026/11/13"), None);
        assert_eq!(ymd(day(2026, 2, 28)), "2026-02-28");
        assert_eq!(ymd(day(2024, 2, 29)), "2024-02-29");
        assert_eq!(iso_day("2026-13-01"), None);
    }

    #[test]
    fn last_full_upgrade_from_pacman_log() {
        let log = "[2026-10-01T09:00:00+0000] [PACMAN] starting full system upgrade\n\
                   [2026-11-02T10:11:12+0000] [PACMAN] starting full system upgrade\n\
                   [2026-11-05T08:00:00+0000] [ALPM] installed foo (1-1)\n";
        assert_eq!(last_upgrade(log), Some(day(2026, 11, 2)));
        assert_eq!(last_upgrade("[2026-11-05T08:00:00+0000] [ALPM] installed foo\n"), None);
    }
}
