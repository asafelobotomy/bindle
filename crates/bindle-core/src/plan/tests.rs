//! Unit tests for `plan::resolve`.

use super::*;

fn facts() -> Facts {
    Facts {
        distro_id: "arch".into(),
        cpu_vendor: "amd".into(),
        gpus: vec!["amd".into()],
        ram_mib: 16000,
        ssd: true,
        virt: "none".into(),
        user: "tester".into(),
        ..Default::default()
    }
}

fn plan(sel: Selection, f: &Facts) -> Result<Plan> {
    resolve(&Catalog::arch().unwrap(), f, &sel, &|_| None)
}

#[test]
fn gaming_profile_enables_multilib_and_lib32_drivers() {
    let p = plan(Selection { profile: Some("gaming".into()), ..Default::default() }, &facts()).unwrap();
    let Step::EditPacmanConf { enable_repos, .. } = &p.steps[0] else { panic!("pacman edit first") };
    assert_eq!(enable_repos, &["multilib"]);
    let pkgs = p.steps.iter().find_map(|s| match s {
        Step::InstallPackages { packages } => Some(packages),
        _ => None,
    });
    let pkgs = pkgs.unwrap();
    assert!(pkgs.contains(&"steam".into()) && pkgs.contains(&"lib32-vulkan-radeon".into()));
    assert!(pkgs.contains(&"amd-ucode".into()));
}

#[test]
fn requires_are_pulled_in_and_reported() {
    let sel = Selection { include: vec!["vscode".into()], ..Default::default() };
    let p = plan(sel, &facts()).unwrap();
    assert!(p.pulled_in.contains(&("aur-helper".into(), "vscode".into())));
    assert!(p.steps.iter().any(|s| matches!(s, Step::BootstrapAurHelper { .. })));
}

#[test]
fn conflicts_are_rejected() {
    let sel = Selection { include: vec!["ufw".into(), "firewalld".into()], ..Default::default() };
    assert!(plan(sel, &facts()).unwrap_err().to_string().contains("conflicts"));
}

#[test]
fn installed_conflicting_packages_skip_the_module() {
    let mut f = facts();
    f.laptop = true;
    f.installed.extend(["rust", "pulseaudio", "power-profiles-daemon"].map(String::from));
    let sel = Selection { profile: Some("developer".into()), include: vec!["tlp".into()], ..Default::default() };
    let p = plan(sel, &f).unwrap();
    for (id, pkg) in [("rustup", "rust"), ("audio-pipewire", "pulseaudio"), ("tlp", "power-profiles-daemon")] {
        assert!(p.skipped.iter().any(|(m, why)| m == id && why.contains(pkg)), "{id}: {:?}", p.skipped);
        assert!(!p.modules.contains(&id.to_string()), "{id}");
    }
    let json = serde_json::to_string(&p.steps).unwrap();
    for pkg in ["\"rustup\"", "\"pipewire-pulse\"", "\"tlp-pd\""] {
        assert!(!json.contains(pkg), "{pkg} still planned");
    }
    assert!(!json.contains("rustup\",\"default"), "user command of a skipped module");
}

#[test]
fn replacing_conflicts_removes_them_right_before_installing() {
    let mut f = facts();
    f.installed.extend(["rust", "pulseaudio", "pulseaudio-bluetooth", "jack2"].map(String::from));
    let sel = Selection { profile: Some("developer".into()), replace_conflicts: true, ..Default::default() };
    let p = plan(sel, &f).unwrap();
    assert!(p.modules.contains(&"rustup".into()) && p.modules.contains(&"audio-pipewire".into()));
    assert!(p.replaced.contains(&("rustup".into(), "rust".into())));
    let removed = p.steps.iter().position(|s| matches!(s, Step::RemovePackages { .. })).expect("remove step");
    let Step::RemovePackages { packages } = &p.steps[removed] else { unreachable!() };
    assert_eq!(packages, &["pulseaudio", "pulseaudio-bluetooth", "jack2", "rust"].map(String::from));
    assert!(matches!(p.steps[removed + 1], Step::InstallPackages { .. }), "install must follow removal");
    // Nothing installed that conflicts: no removal step.
    let sel = Selection { profile: Some("developer".into()), replace_conflicts: true, ..Default::default() };
    assert!(!plan(sel, &facts()).unwrap().steps.iter().any(|s| matches!(s, Step::RemovePackages { .. })));
}

#[test]
fn excluding_a_requirement_fails() {
    let sel = Selection { profile: Some("gaming".into()), exclude: vec!["multilib".into()], ..Default::default() };
    assert!(plan(sel, &facts()).is_err());
}

#[test]
fn unmet_conditions_skip_dependents() {
    let mut f = facts();
    f.virt = "kvm".into();
    let p = plan(Selection::default(), &f).unwrap();
    assert!(p.skipped.iter().any(|(id, _)| id == "microcode"));
}

#[test]
fn installer_choices_are_respected() {
    let mut f = facts();
    f.install.firewall = Some("firewalld".into());
    f.install.bluetooth = Some(true);
    f.install.installer = Some("archinstall".into());
    f.bluetooth = true;
    f.enabled_units.insert("paccache.timer".into());
    let p = plan(Selection { profile: Some("desktop".into()), ..Default::default() }, &f).unwrap();
    assert!(p.provided.iter().any(|(id, why)| id == "bluetooth" && why.contains("archinstall")));
    assert!(p.skipped.iter().any(|(id, why)| id == "ufw" && why.contains("firewalld")));
    let json = serde_json::to_string(&p.steps).unwrap();
    assert!(!json.contains("paccache.timer") && !json.contains("\"ufw\""));
}

#[test]
fn pacman_conf_edits_are_idempotent() {
    let conf = "[options]\nColor\nParallelDownloads = 10\n#VerbosePkgLists\n".to_string();
    let current = move |p: &str| (p == "/etc/pacman.conf").then(|| conf.clone());
    let p = resolve(&Catalog::arch().unwrap(), &facts(), &Selection::default(), &current).unwrap();
    let Step::EditPacmanConf { options, .. } = &p.steps[0] else { panic!("expected pacman edit") };
    assert_eq!(options.keys().collect::<Vec<_>>(), ["VerbosePkgLists"]);
}

#[test]
fn installed_packages_and_identical_files_are_skipped() {
    let mut f = facts();
    f.installed.insert("reflector".into());
    let catalog = Catalog::arch().unwrap();
    let reflector = catalog.module("mirrors").unwrap().files[0].clone();
    let current = move |p: &str| (p == reflector.path).then(|| reflector.content.clone());
    let p = resolve(&catalog, &f, &Selection::default(), &current).unwrap();
    let json = serde_json::to_string(&p.steps).unwrap();
    assert!(!json.contains("\"reflector\""));
    assert!(!json.contains("reflector.conf"));
}

fn root_argvs(p: &Plan, module: &str) -> Vec<Vec<String>> {
    p.steps
        .iter()
        .filter_map(|s| match s {
            Step::RunAsRoot { module: m, argv } if m == module => Some(argv.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn done_commands_and_groups_are_dropped() {
    let mut f = facts();
    f.root_fs = "btrfs".into();
    let sel = Selection { include: vec!["snapper".into(), "ufw".into(), "docker".into()], ..Default::default() };
    let p = plan(sel.clone(), &f).unwrap();
    assert_eq!(root_argvs(&p, "snapper").len(), 1);
    assert_eq!(root_argvs(&p, "ufw").len(), 3, "no kdeconnect rules without KDE Connect");
    assert!(p.steps.contains(&Step::AddToGroups { user: "tester".into(), groups: vec!["docker".into()] }));

    f.snapshots_mount = true;
    let p = plan(sel.clone(), &f).unwrap();
    let snapper = root_argvs(&p, "snapper");
    assert_eq!(snapper.first().unwrap(), &["umount", "/.snapshots"]);
    assert_eq!(snapper.len(), 6);

    f.user_groups.insert("docker".into());
    f.installed.insert("kdeconnect".into());
    let done = |p: &str| match p {
        "/etc/conf.d/snapper" => Some("SNAPPER_CONFIGS=\"root\"\n".to_string()),
        "/etc/ufw/ufw.conf" => Some("ENABLED=yes\n".to_string()),
        _ => None,
    };
    let p = resolve(&Catalog::arch().unwrap(), &f, &sel, &done).unwrap();
    assert!(root_argvs(&p, "snapper").is_empty());
    let ufw = root_argvs(&p, "ufw");
    assert_eq!(ufw.len(), 2, "only the KDE Connect rules remain: {ufw:?}");
    assert!(!p.steps.iter().any(|s| matches!(s, Step::AddToGroups { .. })));
}

#[test]
fn builtin_units_are_enabled() {
    let mut f = facts();
    f.gpus = vec!["amd".into(), "nvidia".into()];
    let p = plan(Selection::default(), &f).unwrap();
    let units = p.steps.iter().find_map(|s| match s {
        Step::EnableUnits { units } => Some(units.clone()),
        _ => None,
    });
    assert!(units.unwrap().contains(&"switcheroo-control.service".to_string()));
}

#[test]
fn arch_only_modules_are_skipped_on_derivatives() {
    let mut f = facts();
    f.distro_id = "manjaro".into();
    f.distro_like = vec!["arch".into()];
    let sel = Selection { include: vec!["kernel-zen".into()], ..Default::default() };
    let p = plan(sel, &f).unwrap();
    for id in ["mirrors", "kernel-zen"] {
        assert!(p.skipped.iter().any(|(m, why)| m == id && why.contains("distro:arch")), "{id}: {:?}", p.skipped);
    }
    assert!(!serde_json::to_string(&p.steps).unwrap().contains("reflector"));
}
