//! Leaving single changes out of a module: a package, an AUR package, a
//! file or a unit (`Change::key`). The plan drops their steps, and the
//! module's recorded content keeps the old value for them, so a later "What's
//! new" shows them again. Everything else in a module (groups, commands,
//! pacman.conf, release pins) goes with the module.

use std::collections::BTreeSet;

use crate::plan::Step;
use crate::plan::content::Content;

/// `kind:value` → (kind, value).
fn split(key: &str) -> Option<(&str, &str)> {
    key.split_once(':')
}

/// `new` with the left-out keys back as they were in `old` (absent when the
/// module had none of it before).
pub fn revert(new: &Content, old: Option<&Content>, left_out: &BTreeSet<String>) -> Content {
    let mut c = new.clone();
    let empty = Content::default();
    let old = old.unwrap_or(&empty);
    for (kind, value) in left_out.iter().filter_map(|k| split(k)) {
        let list = |list: &mut Vec<String>, was: &[String]| {
            if !was.iter().any(|x| x == value) {
                list.retain(|x| x != value);
            }
        };
        match kind {
            "package" => list(&mut c.packages, &old.packages),
            "aur" => list(&mut c.aur, &old.aur),
            "unit" => list(&mut c.units, &old.units),
            "file" => match old.files.get(value) {
                Some(was) => {
                    c.files.insert(value.into(), was.clone());
                }
                None => {
                    c.files.remove(value);
                }
            },
            _ => {}
        }
    }
    c
}

/// The plan's steps without the left-out packages, files and units.
pub fn filter_steps(steps: Vec<Step>, left_out: &BTreeSet<String>) -> Vec<Step> {
    let has = |kind: &str, v: &str| left_out.contains(&format!("{kind}:{v}"));
    steps
        .into_iter()
        .filter_map(|s| match s {
            Step::InstallPackages { packages } => {
                let packages: Vec<String> = packages.into_iter().filter(|p| !has("package", p)).collect();
                (!packages.is_empty()).then_some(Step::InstallPackages { packages })
            }
            Step::InstallAur { packages } => {
                let packages: Vec<String> = packages.into_iter().filter(|p| !has("aur", p)).collect();
                (!packages.is_empty()).then_some(Step::InstallAur { packages })
            }
            Step::EnableUnits { units } => {
                let units: Vec<String> = units.into_iter().filter(|u| !has("unit", u)).collect();
                (!units.is_empty()).then_some(Step::EnableUnits { units })
            }
            Step::WriteFile { file, .. } if has("file", &file.path) => None,
            other => Some(other),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::FileSpec;

    fn keys(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn left_out_parts_keep_their_old_value() {
        let old = Content {
            packages: vec!["cups".into()],
            files: [("/etc/a".to_string(), "old".to_string())].into(),
            ..Default::default()
        };
        let new = Content {
            packages: vec!["cups".into(), "gutenprint".into()],
            files: [("/etc/a".to_string(), "new".to_string()), ("/etc/b".to_string(), "b".to_string())].into(),
            units: vec!["cups.socket".into()],
            ..Default::default()
        };
        let r = revert(&new, Some(&old), &keys(&["package:gutenprint", "file:/etc/a", "file:/etc/b"]));
        assert_eq!(r.packages, ["cups"]);
        assert_eq!(r.files.get("/etc/a").map(String::as_str), Some("old"));
        assert!(!r.files.contains_key("/etc/b"));
        assert_eq!(r.units, ["cups.socket"], "not left out");
        assert_eq!(revert(&new, None, &keys(&["unit:cups.socket"])).units, Vec::<String>::new());
    }

    #[test]
    fn steps_lose_only_the_left_out_parts() {
        let file = |p: &str| FileSpec { path: p.into(), content: String::new(), mode: 0o644 };
        let steps = vec![
            Step::InstallPackages { packages: vec!["cups".into(), "gutenprint".into()] },
            Step::InstallAur { packages: vec!["x".into()] },
            Step::WriteFile { module: "m".into(), file: file("/etc/a") },
            Step::WriteFile { module: "m".into(), file: file("/etc/b") },
            Step::EnableUnits { units: vec!["cups.socket".into()] },
        ];
        let out = filter_steps(steps, &keys(&["package:gutenprint", "aur:x", "file:/etc/a", "unit:cups.socket"]));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Step::InstallPackages { packages: vec!["cups".into()] });
        assert!(matches!(&out[1], Step::WriteFile { file, .. } if file.path == "/etc/b"));
    }
}
