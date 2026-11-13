//! Line-level `/etc/fstab` handling for drives mounted at startup.
//!
//! fstab has no drop-in directory, so Bindle keeps its lines in one marked
//! block at the end of the file and never touches a line outside it. Adding a
//! drive that is already in the block replaces its line, so a re-run never
//! duplicates an entry.

use serde::{Deserialize, Serialize};

pub const PATH: &str = "/etc/fstab";
pub const BEGIN: &str = "# BEGIN Managed by Bindle: drives mounted at startup";
pub const END: &str = "# END Managed by Bindle";

/// One drive's line. Always mounted by `UUID=`: device names change between boots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub uuid: String,
    pub dir: String,
    pub fstype: String,
    pub options: String,
    /// fsck order (`fs_passno`): 2 for ext2/3/4, 0 for the rest.
    pub pass: u8,
    /// Not mounted anywhere right now: mount it as soon as the line is written.
    #[serde(default)]
    pub mount_now: bool,
}

impl Entry {
    /// The exact line written to fstab.
    pub fn line(&self) -> String {
        format!("UUID={}\t{}\t{}\t{}\t0 {}", self.uuid, self.dir, self.fstype, self.options, self.pass)
    }

    /// Every field is one plain word: nothing here can add a second line or field.
    pub fn check(&self) -> Result<(), String> {
        let plain =
            |s: &str, extra: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c));
        let dir_ok = self.dir.starts_with('/') && plain(&self.dir, "/._-") && !self.dir.split('/').any(|p| p == "..");
        if plain(&self.uuid, "-")
            && dir_ok
            && plain(&self.fstype, "")
            && plain(&self.options, "=,.:-_")
            && self.pass <= 2
        {
            Ok(())
        } else {
            Err(format!("refusing fstab entry `{}`", self.line().replace('\t', " ")))
        }
    }
}

/// A filesystem line already in fstab.
#[derive(Debug, PartialEq, Eq)]
pub struct Line<'a> {
    /// First field: `UUID=…`, `LABEL=…`, `/dev/…`.
    pub spec: &'a str,
    pub dir: &'a str,
    /// Inside Bindle's block.
    pub managed: bool,
    pub text: &'a str,
}

/// Every filesystem line (comments and blanks skipped), in file order.
pub fn lines(text: &str) -> Vec<Line<'_>> {
    let mut managed = false;
    let mut out = Vec::new();
    for l in text.lines() {
        match l.trim() {
            BEGIN => managed = true,
            END => managed = false,
            t if t.is_empty() || t.starts_with('#') => {}
            t => {
                let mut fields = t.split_whitespace();
                if let (Some(spec), Some(dir)) = (fields.next(), fields.next()) {
                    out.push(Line { spec, dir, managed, text: l });
                }
            }
        }
    }
    out
}

/// Whether a line's first field names this UUID (`UUID=` or a by-uuid path).
pub fn names_uuid(spec: &str, uuid: &str) -> bool {
    let named = spec.strip_prefix("UUID=").or_else(|| spec.strip_prefix("/dev/disk/by-uuid/"));
    named.is_some_and(|u| u.trim_matches('"').eq_ignore_ascii_case(uuid))
}

/// `text` with `entries` in Bindle's block: a drive already there has its
/// line replaced, a new one is added, everything else stays byte for byte.
pub fn merge(text: &str, entries: &[Entry]) -> String {
    let mut out: Vec<String> = text.lines().map(String::from).collect();
    let begin = out.iter().position(|l| l.trim() == BEGIN);
    let (begin, mut end) = match begin {
        Some(b) => {
            // A block someone cut the end off: it runs to the end of the file.
            let end = match out[b..].iter().position(|l| l.trim() == END) {
                Some(e) => b + e,
                None => {
                    out.push(END.into());
                    out.len() - 1
                }
            };
            (b, end)
        }
        None => {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.extend([BEGIN.to_string(), END.to_string()]);
            (out.len() - 2, out.len() - 1)
        }
    };
    for e in entries {
        let same = |l: &String| l.split_whitespace().next().is_some_and(|spec| names_uuid(spec, &e.uuid));
        match out[begin + 1..end].iter().position(same) {
            Some(i) => out[begin + 1 + i] = e.line(),
            None => {
                out.insert(end, e.line());
                end += 1;
            }
        }
    }
    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYSTEM: &str =
        "# /dev/nvme0n1p2\nUUID=aaaa-1\t/\tbtrfs\trw,subvol=/@\t0 0\n\n/swap/swapfile none swap sw 0 0\n";

    fn entry(uuid: &str, dir: &str) -> Entry {
        Entry {
            uuid: uuid.into(),
            dir: dir.into(),
            fstype: "ext4".into(),
            options: "defaults,nofail".into(),
            pass: 2,
            mount_now: false,
        }
    }

    #[test]
    fn adds_a_block_and_leaves_the_rest_alone() {
        let once = merge(SYSTEM, &[entry("bbbb-2", "/mnt/Data")]);
        assert!(once.starts_with(SYSTEM));
        let block = format!("\n{BEGIN}\nUUID=bbbb-2\t/mnt/Data\text4\tdefaults,nofail\t0 2\n{END}\n");
        assert_eq!(once, format!("{SYSTEM}{block}"));
        // Same entry again: nothing changes. Another drive: appended in the block.
        assert_eq!(merge(&once, &[entry("bbbb-2", "/mnt/Data")]), once);
        let two = merge(&once, &[entry("cccc-3", "/mnt/Games")]);
        assert_eq!(two.matches(BEGIN).count(), 1);
        let managed: Vec<&str> = lines(&two).into_iter().filter(|l| l.managed).map(|l| l.dir).collect();
        assert_eq!(managed, ["/mnt/Data", "/mnt/Games"]);
        assert_eq!(lines(&two).iter().filter(|l| !l.managed).count(), 2);
    }

    #[test]
    fn replaces_a_drives_line_instead_of_duplicating_it() {
        let once = merge(SYSTEM, &[entry("bbbb-2", "/mnt/Data"), entry("cccc-3", "/mnt/Games")]);
        let moved = Entry { options: "defaults,noatime,nofail".into(), ..entry("BBBB-2", "/mnt/Data") };
        let again = merge(&once, std::slice::from_ref(&moved));
        assert_eq!(again.matches("/mnt/Data").count(), 1);
        assert!(again.contains(&moved.line()) && again.contains("UUID=cccc-3"));
        // A user's own line for the same drive, outside the block, is never edited.
        let own = format!("UUID=bbbb-2 /data ext4 defaults 0 2\n{once}");
        assert!(merge(&own, &[entry("bbbb-2", "/mnt/Data")]).starts_with("UUID=bbbb-2 /data ext4 defaults 0 2\n"));
    }

    #[test]
    fn handles_an_empty_file_a_missing_newline_and_a_cut_block() {
        assert_eq!(merge("", &[entry("b", "/mnt/b")]), format!("{BEGIN}\n{}\n{END}\n", entry("b", "/mnt/b").line()));
        let no_newline = merge("UUID=a / ext4 defaults 0 1", &[entry("b", "/mnt/b")]);
        assert!(no_newline.starts_with("UUID=a / ext4 defaults 0 1\n\n# BEGIN"));
        let cut = format!("{BEGIN}\n{}\n", entry("b", "/mnt/b").line());
        let fixed = merge(&cut, &[entry("c", "/mnt/c")]);
        assert!(fixed.ends_with(&format!("{}\n{END}\n", entry("c", "/mnt/c").line())), "{fixed}");
    }

    #[test]
    fn entries_are_single_plain_fields() {
        assert!(entry("1A2B-3C4D", "/mnt/My_Disk.1").check().is_ok());
        assert!(entry("a b", "/mnt/x").check().is_err());
        assert!(entry("a", "/mnt/x y").check().is_err());
        assert!(entry("a", "/mnt/../etc").check().is_err());
        assert!(entry("a", "mnt/x").check().is_err());
        assert!(Entry { options: "defaults\n/dev/sda / ext4".into(), ..entry("a", "/mnt/x") }.check().is_err());
        assert!(names_uuid("UUID=AbC", "abc") && names_uuid("/dev/disk/by-uuid/abc", "abc"));
        assert!(!names_uuid("LABEL=abc", "abc"));
    }
}
