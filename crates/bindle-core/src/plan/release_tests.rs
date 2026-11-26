//! Apps installed from pinned GitHub releases (none of uQ's: the shipped
//! catalog pins none, so these use a small one of their own), and
//! asafelobotomy's own apps, which come from the AUR only.

use super::*;

fn facts() -> Facts {
    Facts { distro_id: "arch".into(), user: "tester".into(), home: "/home/tester".into(), ..Default::default() }
}

fn plan(sel: Selection, f: &Facts) -> Result<Plan> {
    resolve(&Catalog::arch().unwrap(), f, &sel, &|_| None)
}

/// One third-party AppImage pinned to a release.
fn pinned_catalog() -> Catalog {
    let file = r#"
        [category]
        id = "apps"
        name = "Apps"
        description = ""
        [[module]]
        id = "reshadelinux"
        name = "ReShadeLinux"
        description = ""
        packages = ["fuse2"]
        [[module.release]]
        repo = "example/reshadelinux"
        tag = "v1.4.1"
        asset = "reshadelinux-1.4.1-x86_64.AppImage"
        sha256 = "974b66a99d9c3447161a111a61deca49381d587330494dedc627107e64ec0bae"
        name = "reshadelinux"
    "#;
    let profiles = "[[profile]]\nid = \"p\"\nname = \"P\"\ndescription = \"\"\n";
    Catalog::from_sources("arch", &[("apps", file)], profiles).unwrap()
}

#[test]
fn release_assets_are_planned_until_installed() {
    let f = facts();
    let cat = pinned_catalog();
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
    let p = resolve(&cat, &f, &sel, &|_| None).unwrap();
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
    let p = resolve(&cat, &f, &sel, &entry).unwrap();
    assert!(assets(&p).is_empty(), "{:?}", p.steps);
    // An older AppImage is replaced by the pinned one.
    let old = |path: &str| {
        path.ends_with("bindle-reshadelinux.desktop")
            .then(|| "X-Bindle-Asset=reshadelinux-0.1.0-x86_64.AppImage\n".into())
    };
    assert_eq!(assets(&resolve(&cat, &f, &sel, &old).unwrap()), ["reshadelinux"]);
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
    // The polkit action its Grant copied in by hand goes before: the package ships it.
    let unowned = p.steps.iter().position(|s| matches!(s, Step::RunAsRoot { module, .. } if module == "emobie"));
    assert!(unowned.is_some_and(|i| i < at_aur), "{:?}", p.steps);
    let installed = |path: &str| (path == "/usr/bin/emobie").then(String::new);
    assert!(cleanup(&resolve(&Catalog::arch().unwrap(), &f, &sel, &installed).unwrap()).is_none());
}

#[test]
fn the_catalog_offers_one_aur_package_per_own_app_and_no_release_pins() {
    let c = Catalog::arch().unwrap();
    assert!(c.modules.iter().all(|m| m.releases.is_empty()), "Bindle installs nothing from GitHub releases");
    // `x` and `x-bin` (or `x-git`) are the same app: only one of them is offered.
    let aur: Vec<&str> = c.modules.iter().flat_map(|m| m.aur.iter().map(String::as_str)).collect();
    for name in &aur {
        let base = name.trim_end_matches("-bin").trim_end_matches("-git");
        let twins: Vec<&&str> =
            aur.iter().filter(|n| n.trim_end_matches("-bin").trim_end_matches("-git") == base).collect();
        assert!(twins.len() == 1 || twins.iter().all(|t| *t == name), "{base} is offered as {twins:?}");
    }
    // Bindle itself comes from its AUR package once an AUR helper is in.
    let bindle = c.module("bindle").expect("a `bindle` module");
    assert_eq!(bindle.aur, ["bindle"]);
}
