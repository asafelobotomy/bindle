//! Drives mounted at startup: the plan's `/etc/fstab` step.

use super::{Selection, Step};
use crate::builtins::drives::ID_PREFIX;
use crate::catalog::Catalog;
use crate::fstab::{self, Entry};

/// A drive module of a drive that isn't connected now (a saved selection
/// replayed later, or on another machine): skipped, not an error.
pub(super) fn absent(catalog: &Catalog, id: &str) -> bool {
    id.starts_with(ID_PREFIX) && catalog.module(id).is_none()
}

pub(super) fn skipped(catalog: &Catalog, selection: &Selection) -> Vec<(String, String)> {
    let gone = selection.include.iter().filter(|id| absent(catalog, id) && !selection.exclude.contains(id));
    gone.map(|id| (id.clone(), "that drive is not connected".to_string())).collect()
}

/// The lines still to write: one already in Bindle's block, exactly as
/// planned, is done.
pub(super) fn step(entries: Vec<Entry>, current_file: &dyn Fn(&str) -> Option<String>) -> Option<Step> {
    let now = current_file(fstab::PATH).unwrap_or_default();
    let have: Vec<&str> = fstab::lines(&now).into_iter().filter(|l| l.managed).map(|l| l.text).collect();
    let entries: Vec<Entry> = entries.into_iter().filter(|e| !have.contains(&e.line().as_str())).collect();
    (!entries.is_empty()).then_some(Step::MountAtBoot { entries })
}

#[cfg(test)]
mod tests {
    use super::super::resolve;
    use super::*;
    use crate::detect::{Drive, Facts};

    fn facts() -> Facts {
        let drive = |device: &str, uuid: &str, fstype: &str, label: &str| Drive {
            device: device.into(),
            uuid: uuid.into(),
            fstype: fstype.into(),
            label: label.into(),
            ..Default::default()
        };
        Facts {
            distro_id: "arch".into(),
            user: "tester".into(),
            uid: 1000,
            gid: 1000,
            drives: vec![drive("/dev/sda1", "aaaa", "ext4", "Data"), drive("/dev/sdb1", "BBBB", "ntfs", "Games")],
            ..Default::default()
        }
    }

    fn plan(f: &Facts, with: &[&str], fstab_now: &str) -> super::super::Plan {
        let catalog = Catalog::arch().unwrap().with_drives(f);
        let sel = Selection { include: with.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        let read = |p: &str| (p == fstab::PATH).then(|| fstab_now.to_string());
        resolve(&catalog, f, &sel, &read).unwrap()
    }

    fn entries(p: &super::super::Plan) -> Vec<Entry> {
        p.steps
            .iter()
            .flat_map(|s| match s {
                Step::MountAtBoot { entries } => entries.clone(),
                _ => Vec::new(),
            })
            .collect()
    }

    #[test]
    fn selected_drives_become_one_fstab_step_with_their_exact_lines() {
        let f = facts();
        assert!(entries(&plan(&f, &[], "")).is_empty(), "no drive is mounted unless chosen");
        let p = plan(&f, &["mount-aaaa", "mount-bbbb"], "UUID=root / ext4 rw 0 1\n");
        assert_eq!(p.steps.iter().filter(|s| matches!(s, Step::MountAtBoot { .. })).count(), 1);
        let lines: Vec<String> = entries(&p).iter().map(Entry::line).collect();
        assert_eq!(
            lines,
            [
                "UUID=aaaa\t/mnt/Data\text4\tdefaults,nofail,x-systemd.device-timeout=30s\t0 2",
                "UUID=BBBB\t/mnt/Games\tntfs3\tuid=1000,gid=1000,umask=022,windows_names,nofail,x-systemd.device-timeout=30s\t0 0",
            ]
        );
        assert!(p.modules.contains(&"mount-aaaa".to_string()));
        assert!(p.notes.iter().any(|(id, n)| id == "mount-bbbb" && n.contains("Fast Startup")));
    }

    #[test]
    fn applying_again_plans_nothing_and_a_new_drive_plans_only_itself() {
        let f = facts();
        let first = entries(&plan(&f, &["mount-aaaa"], ""));
        let written = fstab::merge("UUID=root / ext4 rw 0 1\n", &first);
        assert!(entries(&plan(&f, &["mount-aaaa"], &written)).is_empty());
        let more = entries(&plan(&f, &["mount-aaaa", "mount-bbbb"], &written));
        assert_eq!(more.iter().map(|e| e.dir.as_str()).collect::<Vec<_>>(), ["/mnt/Games"]);
        // The same line outside Bindle's block is someone else's: not counted as done.
        let own = first[0].line();
        assert_eq!(entries(&plan(&f, &["mount-aaaa"], &own)).len(), 1);
    }

    #[test]
    fn a_drive_that_is_gone_or_unusable_is_skipped_not_an_error() {
        let mut f = facts();
        f.drives[1].blocked = Some("already in /etc/fstab, mounted at /games".into());
        let p = plan(&f, &["mount-bbbb", "mount-1234-gone"], "");
        assert!(entries(&p).is_empty());
        let why = |id: &str| p.skipped.iter().find(|(m, _)| m == id).map(|(_, w)| w.as_str());
        assert_eq!(why("mount-bbbb"), Some("already in /etc/fstab, mounted at /games"));
        assert_eq!(why("mount-1234-gone"), Some("that drive is not connected"));
        // Anything else unknown is still a typo.
        let catalog = Catalog::arch().unwrap();
        let sel = Selection { include: vec!["no-such-module".into()], ..Default::default() };
        assert!(resolve(&catalog, &f, &sel, &|_| None).is_err());
    }
}
