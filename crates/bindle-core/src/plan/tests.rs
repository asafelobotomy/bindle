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
    assert_eq!(snapper.len(), 2, "free the mount + create config, then remount");
    assert!(snapper[0][2].contains("create-config") && snapper[1][2].contains("mount /.snapshots"));

    f.user_groups.insert("docker".into());
    f.installed.insert("kdeconnect".into());
    let done = |p: &str| match p {
        "/etc/conf.d/snapper" => Some("SNAPPER_CONFIGS=\"root\"\n".to_string()),
        "/proc/self/mounts" => Some("/dev/sda2 /.snapshots btrfs rw 0 0\n".to_string()),
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
        assert!(p.skipped.iter().any(|(m, why)| m == id && why == "needs Arch Linux"), "{id}: {:?}", p.skipped);
    }
    assert!(!serde_json::to_string(&p.steps).unwrap().contains("reflector"));
}

#[test]
fn snapper_resumes_after_a_partial_failure() {
    // Config written, but the remount never happened: only the remount runs.
    let mut f = facts();
    f.root_fs = "btrfs".into();
    f.snapshots_mount = true;
    let sel = Selection { include: vec!["snapper".into()], ..Default::default() };
    let current = |p: &str| (p == "/etc/conf.d/snapper").then(|| "SNAPPER_CONFIGS=\"root\"\n".to_string());
    let p = resolve(&Catalog::arch().unwrap(), &f, &sel, &current).unwrap();
    let snapper = root_argvs(&p, "snapper");
    assert_eq!(snapper.len(), 1);
    assert!(snapper[0][2].contains("mount /.snapshots"));
}

#[test]
fn release_assets_are_planned_until_installed() {
    let mut f = facts();
    f.home = "/home/tester".into();
    let sel = Selection { include: vec!["hauler".into(), "emobie".into()], ..Default::default() };
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
    assert_eq!(assets(&p), ["emobie", "hauler"]);
    let Some(Step::InstallReleases { assets: a }) = p.steps.iter().find(|s| matches!(s, Step::InstallReleases { .. }))
    else {
        unreachable!()
    };
    assert_eq!(a[0].title, "emobie (emoji palette)", "title comes from the module");
    // hauler's package is installed; emobie's desktop entry names this asset.
    f.installed.insert("hauler".into());
    let entry = |path: &str| {
        (path == "/home/tester/.local/share/applications/bindle-emobie.desktop")
            .then(|| "X-Bindle-Asset=emobie_0.8.2_amd64.AppImage\n".to_string())
    };
    let p = resolve(&Catalog::arch().unwrap(), &f, &sel, &entry).unwrap();
    assert!(assets(&p).is_empty(), "{:?}", p.steps);
    // An older AppImage is replaced by the pinned one.
    let old = |path: &str| {
        path.ends_with("bindle-emobie.desktop").then(|| "X-Bindle-Asset=emobie_0.7.0_amd64.AppImage\n".into())
    };
    assert_eq!(assets(&resolve(&Catalog::arch().unwrap(), &f, &sel, &old).unwrap()), ["emobie"]);
}

#[test]
fn uq_module_builds_from_the_aur_then_runs_setup_as_the_user() {
    let mut f = facts();
    f.desktops = vec!["hyprland".into()];
    f.home = "/home/tester".into();
    let sel = Selection { profile: Some("desktop".into()), include: vec!["uq".into()], ..Default::default() };
    let p = plan(sel.clone(), &f).unwrap();
    assert!(p.modules.contains(&"aur-helper".to_string()) && p.modules.contains(&"uq".to_string()));
    let aur = p
        .steps
        .iter()
        .position(|s| matches!(s, Step::InstallAur { packages } if packages == &["uq", "uq-hyprland-pin", "bibata-cursor-theme-bin"]));
    let setup = p.steps.iter().position(|s| matches!(s, Step::RunAsUser { argv, .. } if argv == &["uqctl", "setup"]));
    assert!(aur.unwrap() < setup.unwrap(), "{:?}", p.steps);
    // Exit 3 (the user's own hyprland.lua was kept) is a manual step, not a failure.
    let Step::RunAsUser { manual, .. } = &p.steps[setup.unwrap()] else { unreachable!() };
    assert!(manual.get("3").is_some_and(|m| m.contains("hyprland.lua")) && manual.len() == 1);
    // desktop-essentials rounds out uQ, not the Hyprland it rides on.
    let pkgs = p.steps.iter().find_map(|s| match s {
        Step::InstallPackages { packages } => Some(packages.clone()),
        _ => None,
    });
    let pkgs = pkgs.unwrap();
    assert!(pkgs.contains(&"papirus-icon-theme".into()) && pkgs.contains(&"kitty".into()));
    assert!(!pkgs.contains(&"waybar".into()));
    // Once uqctl has written its stub, setup isn't planned again.
    let stub = "require(\"/usr/share/uq/hypr/base.lua\")\nrequire(\"./uq/generated.lua\")\n";
    let done = resolve(&Catalog::arch().unwrap(), &f, &sel, &|p| {
        (p == "/home/tester/.config/hypr/hyprland.lua").then(|| stub.to_string())
    })
    .unwrap();
    assert!(!done.steps.iter().any(|s| matches!(s, Step::RunAsUser { argv, .. } if argv[0] == "uqctl")));
}
