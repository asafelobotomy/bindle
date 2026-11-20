//! Apps installed from pinned GitHub releases, and asafelobotomy's own apps,
//! which come from the AUR.

use super::*;

fn facts() -> Facts {
    Facts { distro_id: "arch".into(), user: "tester".into(), home: "/home/tester".into(), ..Default::default() }
}

fn plan(sel: Selection, f: &Facts) -> Result<Plan> {
    resolve(&Catalog::arch().unwrap(), f, &sel, &|_| None)
}

#[test]
fn release_assets_are_planned_until_installed() {
    let f = facts();
    let sel = Selection { include: vec!["reshadelinux".into()], ..Default::default() };
    let assets = |p: &Plan| -> Vec<String> {
        p.steps
            .iter()
            .find_map(|s| match s {
                Step::InstallReleases { assets } => Some(assets.iter().map(|a| a.name.clone()).collect()),
                _ => None,
            })
            .unwrap_or_default()
    };
    let p = plan(sel.clone(), &f).unwrap();
    assert_eq!(assets(&p), ["reshadelinux"]);
    let Some(Step::InstallReleases { assets: a }) = p.steps.iter().find(|s| matches!(s, Step::InstallReleases { .. }))
    else {
        unreachable!()
    };
    assert_eq!(a[0].title, "ReShadeLinux", "title comes from the module");
    let pinned = a[0].asset.clone();
    // The desktop entry names this asset: nothing to do.
    let entry = |path: &str| {
        (path == "/home/tester/.local/share/applications/bindle-reshadelinux.desktop")
            .then(|| format!("X-Bindle-Asset={pinned}\n"))
    };
    let p = resolve(&Catalog::arch().unwrap(), &f, &sel, &entry).unwrap();
    assert!(assets(&p).is_empty(), "{:?}", p.steps);
    // An older AppImage is replaced by the pinned one.
    let old = |path: &str| {
        path.ends_with("bindle-reshadelinux.desktop")
            .then(|| "X-Bindle-Asset=reshadelinux-0.1.0-x86_64.AppImage\n".into())
    };
    assert_eq!(assets(&resolve(&Catalog::arch().unwrap(), &f, &sel, &old).unwrap()), ["reshadelinux"]);
}

#[test]
fn own_apps_come_from_the_aur_and_retire_bindles_appimage() {
    let f = facts();
    let sel = Selection { include: vec!["emobie".into(), "hauler".into()], ..Default::default() };
    let p = plan(sel.clone(), &f).unwrap();
    assert!(!p.steps.iter().any(|s| matches!(s, Step::InstallReleases { .. })), "{:?}", p.steps);
    let aur: Vec<String> = p
        .steps
        .iter()
        .find_map(|s| match s {
            Step::InstallAur { packages } => Some(packages.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert!(aur.contains(&"emobie".into()) && aur.contains(&"hauler".into()), "{aur:?}");
    // The AppImage clean-up runs after the AUR install, until the package is in.
    let cleanup = |p: &Plan| {
        p.steps.iter().position(
            |s| matches!(s, Step::RunAsUser { argv, .. } if argv.last().map(String::as_str) == Some("emobie")),
        )
    };
    let at_aur = p.steps.iter().position(|s| matches!(s, Step::InstallAur { .. })).unwrap();
    assert!(cleanup(&p).is_some_and(|i| i > at_aur), "{:?}", p.steps);
    let installed = |path: &str| (path == "/usr/bin/emobie").then(String::new);
    assert!(cleanup(&resolve(&Catalog::arch().unwrap(), &f, &sel, &installed).unwrap()).is_none());
}
