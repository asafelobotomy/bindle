//! AppImages Bindle installed from a pinned GitHub release before its apps
//! moved to the AUR (0.1.49): `~/.local/bin/<name>` and the launcher
//! `~/.local/share/applications/bindle-<name>.desktop` (marked
//! `X-Bindle-Asset=`). Once a package installs the same app, the old copy
//! is a second entry in the app menu, and on a `PATH` with `~/.local/bin`
//! first it shadows the package's program. Pure; the files come in as data.

use std::collections::BTreeSet;

use serde::Serialize;

/// Bindle's launcher for an AppImage it installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The launcher's path.
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Leftover {
    /// The app's name: the launcher is `bindle-<name>.desktop`.
    pub name: String,
    /// The files that go: the launcher, and the AppImage when it is still there.
    pub files: Vec<String>,
    /// A package now installs `/usr/bin/<name>` (else the AppImage itself is gone).
    pub replaced: bool,
}

/// The app's name from `…/bindle-<name>.desktop`.
fn name(path: &str) -> Option<&str> {
    let file = path.rsplit('/').next()?;
    file.strip_prefix("bindle-")?.strip_suffix(".desktop").filter(|n| !n.is_empty() && !n.contains('/'))
}

/// The program a Bindle launcher runs: `Exec="<path>" %U`.
fn program(text: &str) -> Option<&str> {
    let exec = text.lines().find_map(|l| l.strip_prefix("Exec="))?;
    let rest = exec.strip_prefix('"')?;
    rest.split_once('"').map(|(p, _)| p).filter(|p| p.starts_with('/'))
}

/// The leftovers to remove. A launcher counts only when Bindle wrote it
/// (`X-Bindle-Asset=`) and this Bindle no longer pins that app (`pinned`:
/// the names of pinned AppImages, which the Releases stage updates). It
/// goes when its AppImage is gone, or when `packaged(name)` (a package's
/// `/usr/bin/<name>`) says a package has the app now; the AppImage goes
/// with it only when it is the `~/.local/bin/<name>` Bindle wrote. An old
/// copy no package replaces yet stays: it may be the only one.
pub fn find(
    entries: &[Entry],
    home: &str,
    pinned: &BTreeSet<String>,
    exists: &dyn Fn(&str) -> bool,
    packaged: &dyn Fn(&str) -> bool,
) -> Vec<Leftover> {
    let bin = format!("{}/.local/bin/", home.trim_end_matches('/'));
    let mut out = vec![];
    for e in entries {
        let Some(name) = name(&e.path) else { continue };
        if pinned.contains(name) || !e.text.lines().any(|l| l.starts_with("X-Bindle-Asset=")) {
            continue;
        }
        let program = program(&e.text);
        let present = program.is_some_and(exists);
        let replaced = packaged(name);
        if present && !replaced {
            continue;
        }
        let mut files = vec![e.path.clone()];
        if let Some(p) = program.filter(|p| present && *p == format!("{bin}{name}")) {
            files.push(p.to_string());
        }
        out.push(Leftover { name: name.to_string(), files, replaced });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/u";

    fn entry(name: &str, exec: &str, marked: bool) -> Entry {
        let mark = if marked { format!("X-Bindle-Asset={name}-1-x86_64.AppImage\n") } else { String::new() };
        Entry {
            path: format!("{HOME}/.local/share/applications/bindle-{name}.desktop"),
            text: format!(
                "[Desktop Entry]\nType=Application\nName={name}\nExec=\"{exec}\" %U\n# Managed by Bindle\n{mark}"
            ),
        }
    }

    fn run(entries: &[Entry], pinned: &[&str]) -> Vec<Leftover> {
        let pinned = pinned.iter().map(|s| s.to_string()).collect();
        let exists = |p: &str| p.ends_with("/scale_tail") || p.ends_with("/kept");
        let packaged = |n: &str| n == "scale_tail" || n == "gone";
        find(entries, HOME, &pinned, &exists, &packaged)
    }

    #[test]
    fn a_copy_a_package_replaced_goes_with_its_appimage() {
        let got = run(&[entry("scale_tail", "/home/u/.local/bin/scale_tail", true)], &[]);
        assert_eq!(
            got,
            vec![Leftover {
                name: "scale_tail".into(),
                files: vec![
                    "/home/u/.local/share/applications/bindle-scale_tail.desktop".into(),
                    "/home/u/.local/bin/scale_tail".into()
                ],
                replaced: true
            }]
        );
    }

    #[test]
    fn a_launcher_whose_appimage_is_gone_goes_alone() {
        let got = run(&[entry("orphan", "/home/u/.local/bin/orphan", true)], &[]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].files, vec!["/home/u/.local/share/applications/bindle-orphan.desktop"]);
        assert!(!got[0].replaced);
    }

    #[test]
    fn kept_pinned_unmarked_and_foreign_paths() {
        // Still the only copy: stays.
        assert!(run(&[entry("kept", "/home/u/.local/bin/kept", true)], &[]).is_empty());
        // Still pinned: the Releases stage updates it.
        assert!(run(&[entry("scale_tail", "/home/u/.local/bin/scale_tail", true)], &["scale_tail"]).is_empty());
        // Not Bindle's launcher.
        assert!(run(&[entry("scale_tail", "/home/u/.local/bin/scale_tail", false)], &[]).is_empty());
        // An Exec elsewhere than Bindle's ~/.local/bin/<name>: only the launcher goes.
        let got = run(&[entry("scale_tail", "/opt/x/scale_tail", true)], &[]);
        assert_eq!(got[0].files.len(), 1);
        // Not a bindle-<name>.desktop.
        let mut e = entry("gone", "/nowhere", true);
        e.path = format!("{HOME}/.local/share/applications/gone.desktop");
        assert!(run(&[e], &[]).is_empty());
    }
}
