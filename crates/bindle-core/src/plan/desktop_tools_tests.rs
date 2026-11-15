//! The Screenshots module: Print bound in the detected window manager's
//! config only, and left to desktops that bind it themselves.

use super::*;

/// Facts for a machine running `desktop`, its tags as detection derives them.
fn on(desktop: &str) -> Facts {
    let all = crate::desktops::all().unwrap();
    let d = crate::desktops::by_id(&all, desktop).unwrap();
    let mut desktop_tags = vec![format!("toolkit:{}", d.toolkit), format!("display:{}", d.display)];
    if d.everyday.is_some() {
        desktop_tags.push("bare-desktop".into());
    }
    Facts {
        distro_id: "arch".into(),
        user: "tester".into(),
        home: "/home/tester".into(),
        desktops: vec![desktop.into()],
        desktop_tags,
        ..Default::default()
    }
}

fn plan(f: &Facts) -> Plan {
    let sel = Selection { profile: Some("desktop".into()), ..Default::default() };
    resolve(&Catalog::arch().unwrap(), f, &sel, &|_| None).unwrap()
}

fn print_key(p: &Plan) -> Vec<Vec<String>> {
    p.steps
        .iter()
        .filter_map(|s| match s {
            Step::RunAsUser { module, argv, .. } if module == "screenshots" => Some(argv.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn print_is_bound_for_the_window_manager_in_use() {
    for wm in ["sway", "i3", "awesome", "qtile", "bspwm", "niri", "labwc", "hyprland"] {
        let p = plan(&on(wm));
        assert!(p.modules.contains(&"screenshots".to_string()), "{wm}: {:?} skipped {:?}", p.modules, p.skipped);
        assert_eq!(print_key(&p), [["/usr/local/lib/bindle/print-key", wm]], "{wm}");
        let files: Vec<&str> = p
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::WriteFile { file, .. } => Some(file.path.as_str()),
                _ => None,
            })
            .collect();
        assert!(files.contains(&"/usr/local/bin/bindle-screenshot"), "{wm}: {files:?}");
    }
}

#[test]
fn desktops_with_their_own_screenshots_are_left_alone() {
    for desktop in ["uq", "gnome", "plasma", "cosmic", "niri-dms", "river", "xmonad"] {
        assert!(!plan(&on(desktop)).modules.contains(&"screenshots".to_string()), "{desktop}");
    }
}

#[test]
fn a_bound_key_is_not_bound_again() {
    let f = on("sway");
    let sel = Selection { profile: Some("desktop".into()), ..Default::default() };
    let done = |p: &str| (p == "/home/tester/.config/sway/config").then(|| "# BEGIN Bindle: Print …".to_string());
    let p = resolve(&Catalog::arch().unwrap(), &f, &sel, &done).unwrap();
    assert!(print_key(&p).is_empty());
}
