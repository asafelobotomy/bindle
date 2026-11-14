//! Unit tests for the `package_conflicts` rule: what one chosen module
//! replaces, no other module of the plan installs.

use super::*;

fn sway() -> Facts {
    Facts {
        distro_id: "arch".into(),
        cpu_vendor: "amd".into(),
        gpus: vec!["amd".into()],
        ram_mib: 16000,
        ssd: true,
        virt: "none".into(),
        user: "tester".into(),
        home: "/home/tester".into(),
        desktops: vec!["sway".into()],
        desktop_tags: vec!["toolkit:other".into(), "display:wayland".into()],
        ..Default::default()
    }
}

fn installs(p: &Plan) -> Vec<String> {
    p.steps
        .iter()
        .flat_map(|s| match s {
            Step::InstallPackages { packages } | Step::InstallAur { packages } => packages.clone(),
            _ => Vec::new(),
        })
        .collect()
}

fn plan(include: &[&str], f: &Facts) -> Plan {
    let sel = Selection { include: include.iter().map(|s| s.to_string()).collect(), ..Default::default() };
    resolve(&Catalog::arch().unwrap(), f, &sel, &|_| None).unwrap()
}

#[test]
fn desktop_essentials_bring_mako_without_noti_fi() {
    let p = plan(&["desktop-essentials"], &sway());
    assert!(installs(&p).contains(&"mako".to_string()), "{:?}", p.steps);
}

#[test]
fn noti_fi_keeps_mako_out_of_the_desktop_essentials() {
    let p = plan(&["desktop-essentials", "noti-fi"], &sway());
    let pkgs = installs(&p);
    assert!(!pkgs.contains(&"mako".to_string()), "{pkgs:?}");
    assert!(pkgs.contains(&"wl-clipboard".to_string()) && pkgs.contains(&"noti-fi".to_string()), "{pkgs:?}");
    let note = p.notes.iter().find(|(m, n)| m == "noti-fi" && n.contains("`mako`") && n.contains("desktop-essentials"));
    assert!(note.is_some(), "{:?}", p.notes);
    // The note is about mako alone: dunst and swaync were never planned.
    assert!(!p.notes.iter().any(|(_, n)| n.contains("`dunst`") || n.contains("`swaync`")), "{:?}", p.notes);
}

#[test]
fn an_installed_conflict_is_neither_noted_nor_installed() {
    let mut f = sway();
    f.installed.insert("mako".into());
    let sel = Selection {
        include: vec!["desktop-essentials".into(), "noti-fi".into()],
        replace_conflicts: true,
        ..Default::default()
    };
    let p = resolve(&Catalog::arch().unwrap(), &f, &sel, &|_| None).unwrap();
    assert!(!installs(&p).contains(&"mako".to_string()));
    assert!(p.replaced.contains(&("noti-fi".into(), "mako".into())));
    assert!(!p.notes.iter().any(|(_, n)| n.contains("not installing `mako`")), "{:?}", p.notes);
}
