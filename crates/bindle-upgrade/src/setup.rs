//! Bindle's own setup, looked at again: "What's new" (modules this Bindle
//! would add, change or retire compared with what earlier versions set up,
//! from `bindle_run::whatsnew`), files Bindle wrote that changed since by
//! hand, and drives in Bindle's fstab block that aren't there. Only
//! reported: `bindle whats-new` (or the window) reviews and applies.

use std::collections::BTreeSet;
use std::path::Path;

use bindle_core::whatsnew::Kind;
use bindle_core::{Catalog, Facts, Step, fstab};
use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Setup {
    /// A saved selection exists: there is something to compare.
    pub known: bool,
    /// What's new: (kind, name) of each item.
    pub whats_new: Vec<(Kind, String)>,
    /// What changed since Bindle set it up, one line each.
    pub drift: Vec<String>,
}

/// Files the plan would write again for modules whose catalog content is
/// unchanged (What's new covers the others): changed on disk since.
pub fn changed_files(steps: &[Step], unchanged: &BTreeSet<String>) -> Vec<String> {
    steps
        .iter()
        .filter_map(|s| match s {
            Step::WriteFile { module, file } if unchanged.contains(module) => {
                Some(format!("{} changed since Bindle wrote it", file.path))
            }
            _ => None,
        })
        .collect()
}

/// Drives in Bindle's fstab block (mounted at startup) whose UUID is gone.
pub fn missing_drives(fstab_text: &str, uuid_present: &dyn Fn(&str) -> bool) -> Vec<String> {
    fstab::lines(fstab_text)
        .into_iter()
        .filter(|l| l.managed && !l.text.contains("noauto"))
        .filter_map(|l| {
            let uuid = l.spec.strip_prefix("UUID=")?;
            (!uuid_present(uuid)).then(|| format!("the drive for {} (UUID {uuid}) isn't connected", l.dir))
        })
        .collect()
}

/// Look: What's new, and drift.
pub fn look() -> Setup {
    let mut drift = missing_drives(&std::fs::read_to_string(fstab::PATH).unwrap_or_default(), &|u| {
        Path::new("/dev/disk/by-uuid").join(u).exists()
    });
    let Ok(catalog) = Catalog::arch() else { return Setup { drift, ..Default::default() } };
    let facts = Facts::detect();
    let root = Path::new("/");
    let Ok(Some((wn, full))) = bindle_run::whatsnew::look(&catalog, &facts, root, false) else {
        return Setup { drift, ..Default::default() };
    };
    let record = bindle_run::whatsnew::record(root);
    let unchanged: BTreeSet<String> =
        record.modules.iter().filter(|(id, c)| full.contents.get(*id) == Some(c)).map(|(id, _)| id.clone()).collect();
    let current = |p: &str| std::fs::read_to_string(p).ok();
    let sel = bindle_run::whatsnew::saved().map(|(s, _)| s).unwrap_or_default();
    let sel = bindle_core::Selection { replace_conflicts: true, ..sel };
    if let Ok(p) = bindle_core::plan::resolve(&catalog, &facts, &sel, &current) {
        drift.extend(changed_files(&p.steps, &unchanged));
    }
    Setup { known: true, whats_new: wn.items.iter().map(|i| (i.kind, i.name.clone())).collect(), drift }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bindle_core::catalog::FileSpec;

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rewritten_files_of_applied_modules_are_drift() {
        let file = |path: &str| FileSpec { path: path.into(), content: String::new(), mode: 0o644 };
        let steps = [
            Step::WriteFile { module: "zram".into(), file: file("/etc/systemd/zram-generator.conf") },
            Step::WriteFile { module: "new-thing".into(), file: file("/etc/new.conf") },
            Step::RebuildInitramfs,
        ];
        assert_eq!(
            changed_files(&steps, &set(&["zram"])),
            ["/etc/systemd/zram-generator.conf changed since Bindle wrote it"]
        );
    }

    #[test]
    fn drives_that_are_gone() {
        let text = format!(
            "UUID=root / btrfs defaults 0 0\n{}\nUUID=aaaa /mnt/data ext4 nofail 0 2\nUUID=bbbb /mnt/usb exfat noauto,nofail 0 0\n{}\n",
            fstab::BEGIN,
            fstab::END
        );
        assert_eq!(missing_drives(&text, &|u| u == "root"), ["the drive for /mnt/data (UUID aaaa) isn't connected"]);
        assert!(missing_drives(&text, &|_| true).is_empty());
    }
}
