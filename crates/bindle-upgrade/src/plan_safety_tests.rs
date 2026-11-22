//! The rules added for safety and upkeep: Bindle's own update, the
//! preflight holds, rebuilds, uQ, failed services, Bindle's setup, and a
//! restart that isn't safe yet.

use super::*;
use crate::parse::Update;
use crate::selfupdate::SelfState;
use crate::setup::Setup;
use crate::system::{Dkms, Kernel, Mount, Pin, Power, Rebuild};

fn up(name: &str, new: &str) -> Update {
    Update { name: name.into(), old: "1-1".into(), new: new.into() }
}

fn get(plans: &[StagePlan], s: Stage) -> StagePlan {
    plans.iter().find(|p| p.stage == s).unwrap().clone()
}

fn mount(target: &str, fstype: &str, avail_mb: u64) -> Mount {
    Mount { target: target.into(), fstype: fstype.into(), avail_mb }
}

fn held(pl: &StagePlan) -> String {
    match &pl.todo {
        Todo::Hold(why) => why.clone(),
        other => panic!("{:?} isn't held: {other:?}", pl.stage),
    }
}

#[test]
fn a_newer_bindle_updates_itself_where_it_can() {
    let state = |loc: Location| SelfState {
        running: "0.1.8".into(),
        latest: Some(Ok("v0.1.9".into())),
        location: Some(loc),
        just_updated: false,
    };
    let p = Probe { bindle: state(Location::Installed { gui: true }), ..Default::default() };
    let b = get(&plan(&p, &Options::default()), Stage::Bindle);
    assert!(matches!(&b.todo, Todo::Run(a) if matches!(&a[0], Action::SelfUpdate { tag, .. } if tag == "v0.1.9")));
    assert_eq!(b.summary, "Bindle 0.1.9 is out (this is 0.1.8): update, then go on with the new one");
    let p = Probe { bindle: state(Location::Elsewhere("/src/target/debug/bindle".into())), ..Default::default() };
    let b = get(&plan(&p, &Options::default()), Stage::Bindle);
    assert_eq!(b.todo, Todo::Report);
    assert!(b.summary.ends_with("this copy can't update itself"));
    let mut fresh = state(Location::Installed { gui: false });
    fresh.just_updated = true;
    let p = Probe { bindle: fresh, ..Default::default() };
    assert_eq!(get(&plan(&p, &Options::default()), Stage::Bindle).todo, Todo::Nothing);
}

#[test]
fn too_little_room_holds_the_package_stages() {
    let p = Probe {
        pacman: Ok(vec![up("linux-zen", "7.3.5.zen1-1"), up("bash", "5.3-1")]),
        mounts: vec![mount("/", "btrfs", 40_000), mount("/boot", "vfat", 90)],
        snapper_root: true,
        ..Default::default()
    };
    let plans = plan(&p, &Options::default());
    let why = held(&get(&plans, Stage::Pacman));
    assert!(why.starts_with("only 90 MB free on /boot (200 MB needed for the new kernel images)"), "{why}");
    assert_eq!(get(&plans, Stage::Preflight).summary, "1 problem holding the upgrade back");
    assert_eq!(get(&plans, Stage::Snapshot).todo, Todo::Nothing, "no snapshot for an upgrade that waits");
    // Without a kernel update, a small /boot doesn't matter.
    let p = Probe { pacman: Ok(vec![up("bash", "5.3-1")]), ..p };
    assert!(get(&plan(&p, &Options::default()), Stage::Pacman).runs());
}

#[test]
fn space_thresholds_come_from_the_preferences() {
    let p =
        Probe { pacman: Ok(vec![up("bash", "5.3-1")]), mounts: vec![mount("/", "ext4", 2000)], ..Default::default() };
    assert!(matches!(get(&plan(&p, &Options::default()), Stage::Pacman).todo, Todo::Hold(_)));
    let mut o = Options::default();
    o.prefs.merge("[space]\nroot_mb = 1000\n").unwrap();
    assert!(get(&plan(&p, &o), Stage::Pacman).runs());
}

#[test]
fn hyprland_beyond_uqs_pin_waits() {
    let pin = Pin { at_least: Some("0.56.2".into()), below: Some("0.58.0".into()) };
    let p = Probe { pacman: Ok(vec![up("hyprland", "0.58.0-1")]), pin: Some(pin.clone()), ..Default::default() };
    let plans = plan(&p, &Options::default());
    let why = held(&get(&plans, Stage::Pacman));
    assert!(why.contains("pacman would refuse the whole upgrade") && why.contains("wait for uQ"), "{why}");
    let both = Probe { mounts: vec![mount("/", "ext4", 10)], ..p.clone() };
    assert_eq!(get(&plan(&both, &Options::default()), Stage::Preflight).summary, "2 problems holding the upgrade back");
    let p = Probe { aur: Ok(vec![up("uq-hyprland-pin", "0.58.0-1")]), aur_helper: Some("paru".into()), ..p };
    assert!(held(&get(&plan(&p, &Options::default()), Stage::Pacman)).contains("uQ's new pin 0.58.0-1 is on the AUR"));
    let fine = Probe { pacman: Ok(vec![up("hyprland", "0.57.1-1")]), pin: Some(pin), ..Default::default() };
    let plans = plan(&fine, &Options::default());
    assert!(get(&plans, Stage::Pacman).runs());
    assert!(get(&plans, Stage::Preflight).summary.contains("within uQ's tested range"));
}

#[test]
fn a_big_upgrade_on_battery_is_asked_or_held() {
    let p = Probe {
        pacman: Ok(vec![up("linux", "7.0-1")]),
        power: Some(Power { on_battery: true, capacity: Some(80) }),
        ..Default::default()
    };
    let pre = get(&plan(&p, &Options::default()), Stage::Preflight);
    assert_eq!(pre.ask, Some(("Update on battery anyway?".into(), false)));
    assert_eq!(pre.stops, [Stage::Pacman, Stage::Aur, Stage::Releases]);
    let yes = plan(&p, &Options { yes: true, ..Default::default() });
    assert!(held(&get(&yes, Stage::Pacman)).ends_with("or run with --on-battery"));
    let go = plan(&p, &Options { yes: true, on_battery: true, ..Default::default() });
    assert!(get(&go, Stage::Pacman).runs());
    let small = Probe { pacman: Ok(vec![up("bash", "5.3-1")]), ..p };
    assert!(get(&plan(&small, &Options::default()), Stage::Preflight).ask.is_none(), "small and charged");
}

fn after() -> After {
    After { kernel_modules_present: true, aur_helper: Some("paru".into()), ..Default::default() }
}

#[test]
fn broken_aur_packages_are_rebuilt() {
    let now = After {
        rebuild: Some(vec![
            Rebuild { repo: "foreign".into(), name: "python-foo".into(), version: "1.0-1".into() },
            Rebuild { repo: "extra".into(), name: "bar".into(), ..Default::default() },
        ]),
        ..after()
    };
    let r = &after_plans(&now, &now, &["python".into()], &Options::default(), false)[0];
    assert!(matches!(&r.todo, Todo::Run(a) if a == &[Action::UserElevated(
        ["paru", "-S", "--rebuild", "--", "python-foo"].map(String::from).to_vec()
    )]));
    assert!(r.details[1].contains("bar (from extra"));
    assert_eq!(r.summary, "rebuilding python-foo with paru after the library updates");
    assert!(r.ask.is_none(), "not asked mid-run");
    // The window and uQ Settings rebuild too, without a terminal or a build-file review.
    let w = &after_plans(&now, &now, &[], &Options { windowed: true, ..Default::default() }, false)[0];
    assert!(matches!(&w.todo, Todo::Run(a) if a == &[Action::UserElevated(
        ["paru", "-S", "--rebuild", "--noconfirm", "--skipreview", "--", "python-foo"].map(String::from).to_vec()
    )]));
    let check = &after_plans(&now, &now, &[], &Options::default(), true)[0];
    assert_eq!(check.summary, "1 AUR package needs a rebuild (python-foo): paru rebuilds them after the upgrade");
    let missing = &after_plans(&after(), &after(), &["python".into()], &Options::default(), false)[0];
    assert!(missing.summary.starts_with("install rebuild-detector"));
}

#[test]
fn a_package_a_rebuild_did_not_clear_is_not_rebuilt_every_run() {
    // solon64, 2026-11-22: codeql and nvidia-580xx-utils were rebuilt on every run (the driver's
    // rebuild regenerating the UKI each time), and checkrebuild still flagged both right after.
    let flagged = vec![
        Rebuild { repo: "foreign".into(), name: "codeql".into(), version: "2.27.3-1".into() },
        Rebuild { repo: "foreign".into(), name: "nvidia-580xx-utils".into(), version: "580.196.07-1".into() },
    ];
    let stuck = crate::rebuild_state::Stuck::from([
        ("codeql".into(), "2.27.3-1".into()),
        ("nvidia-580xx-utils".into(), "580.196.07-1".into()),
    ]);
    let now = After { rebuild: Some(flagged.clone()), rebuild_stuck: stuck.clone(), ..after() };
    let r = &after_plans(&now, &now, &["harfbuzz".into()], &Options::default(), false)[0];
    assert_eq!(r.todo, Todo::Nothing, "{r:?}");
    assert!(r.summary.starts_with("nothing new needs a rebuild"));
    assert!(r.details.iter().all(|d| d.contains("still flagged after Bindle rebuilt it")));
    // A new version of one is tried again; the other stays out.
    let mut newer = flagged;
    newer[0].version = "2.28.0-1".into();
    let now = After { rebuild: Some(newer), rebuild_stuck: stuck, ..after() };
    let r = &after_plans(&now, &now, &[], &Options::default(), false)[0];
    assert!(
        matches!(&r.todo, Todo::Run(a) if a == &[Action::UserElevated(
            ["paru", "-S", "--rebuild", "--", "codeql"].map(String::from).to_vec()
        )]),
        "{r:?}"
    );
}

#[test]
fn uq_is_set_up_again_and_its_login_screen_kept() {
    let before = After { uq: true, greeter: true, ..after() };
    let now = After { uq: true, greeter: false, ..after() };
    let u = &after_plans(&before, &now, &["uq".into()], &Options::default(), false)[1];
    match &u.todo {
        Todo::Run(a) => {
            assert_eq!(a[0], Action::UqSetup);
            assert_eq!(a[1], Action::Root(["uqctl", "greeter", "enable"].map(String::from).to_vec()));
            assert!(u.summary.ends_with("log out and back in to start the new uQ"));
        }
        other => panic!("{other:?}"),
    }
    let calm = After { uq: true, greeter: true, ..after() };
    assert_eq!(after_plans(&calm, &calm, &[], &Options::default(), false)[1].todo, Todo::Nothing);
}

#[test]
fn only_newly_failed_services_are_news() {
    let before = After { failed: vec!["old.service".into()], ..after() };
    let now = After { failed: vec!["old.service".into(), "bluetooth.service".into()], ..after() };
    let s = &after_plans(&before, &now, &[], &Options::default(), false)[4];
    assert_eq!(s.summary, "1 service failed since the update");
    assert_eq!(s.details[0], "bluetooth.service: systemctl status bluetooth.service");
    assert_eq!(after_plans(&before, &before, &[], &Options::default(), false)[4].todo, Todo::Nothing);
}

#[test]
fn new_features_and_drift_are_reported_not_installed() {
    let now = After {
        setup: Setup {
            known: true,
            whats_new: vec![(bindle_core::whatsnew::Kind::New, "Spell checking".into())],
            drift: vec!["/etc/x changed since Bindle wrote it".into()],
        },
        ..after()
    };
    let s = &after_plans(&now, &now, &[], &Options::default(), false)[5];
    assert_eq!(s.todo, Todo::Report);
    assert_eq!(s.summary, "1 item in What's new: review them with `bindle whats-new`; 1 change since Bindle set it up");
    assert_eq!(s.details[0], "new: Spell checking");
}

#[test]
fn an_unbuilt_dkms_module_means_dont_restart_yet() {
    let kernels = vec![
        Kernel { version: "7.3.4-zen2-1-zen".into(), pkgbase: "linux-zen".into() },
        Kernel { version: "7.3.5-zen1-1-zen".into(), pkgbase: "linux-zen".into() },
    ];
    let dkms = vec![Dkms { module: "nvidia".into(), kernel: "7.3.4-zen2-1-zen".into(), state: "installed".into() }];
    let now =
        After { kernels, dkms, kernel_modules_present: false, running_kernel: "7.3.4-zen2-1-zen".into(), ..after() };
    let check = &after_plans(&now, &now, &["linux-zen".into()], &Options::default(), true)[6];
    assert_eq!(
        held(check),
        "don't restart yet: nvidia isn't built for 7.3.5-zen1-1-zen: pkexec dkms autoinstall -k 7.3.5-zen1-1-zen"
    );
    let fix = &after_plans(&now, &now, &["linux-zen".into()], &Options::default(), false)[6];
    match &fix.todo {
        Todo::Run(a) => {
            assert_eq!(
                a[0],
                Action::Root(["dkms", "autoinstall", "-k", "7.3.5-zen1-1-zen"].map(String::from).to_vec())
            );
            assert_eq!(a[1], Action::Root(vec!["mkinitcpio".into(), "-P".into()]));
        }
        other => panic!("{other:?}"),
    }
    assert!(fix.ask.is_none(), "built without asking mid-run");
    assert!(
        fix.summary.starts_with("builds the missing DKMS modules and boot images (nvidia isn't built"),
        "{}",
        fix.summary
    );
    let images = After { boot: Some(vec!["vmlinuz-linux-zen".into()]), dkms: vec![], ..now.clone() };
    assert!(held(&after_plans(&images, &images, &[], &Options::default(), true)[6]).contains("mkinitcpio -P"));
    let fine = After { dkms: vec![], ..now };
    assert_eq!(after_plans(&fine, &fine, &[], &Options::default(), false)[6].todo, Todo::Report);
}

#[test]
fn an_old_firmware_list_is_mentioned() {
    let p = Probe { firmware: Some(Ok(vec![])), firmware_age: Some(12), ..Default::default() };
    let f = get(&plan(&p, &Options::default()), Stage::Firmware);
    assert_eq!(f.summary, "up to date (firmware list 12 days old; an update run refreshes it)");
    let fresh = Probe { firmware_age: Some(1), ..p };
    assert_eq!(get(&plan(&fresh, &Options::default()), Stage::Firmware).summary, "up to date");
}

#[test]
fn a_failed_check_says_so_instead_of_nothing_to_do() {
    let now = After { unchecked: vec![(Stage::Orphans, "pacman -Qdtq failed (2): database locked".into())], ..after() };
    let plans = after_plans(&now, &now, &[], &Options::default(), false);
    let orphans = plans.iter().find(|p| p.stage == Stage::Orphans).unwrap();
    assert_eq!(orphans.todo, Todo::Report);
    assert!(orphans.summary.starts_with("couldn't check: pacman -Qdtq failed"), "{}", orphans.summary);
    let pacnew = plans.iter().find(|p| p.stage == Stage::Pacfiles).unwrap();
    assert_eq!(pacnew.todo, Todo::Nothing, "other stages are untouched");
}

fn uki(name: &str, kernel: &str) -> bindle_core::uki::Uki {
    bindle_core::uki::Uki { path: format!("/boot/EFI/Linux/{name}.efi"), kernel: kernel.into() }
}

fn zen_with(ukis: Option<Vec<bindle_core::uki::Uki>>) -> After {
    let kernels = vec![Kernel { version: "7.3.5-zen1-1-zen".into(), pkgbase: "linux-zen".into() }];
    After { kernels, ukis, running_kernel: "7.3.5-zen1-1-zen".into(), ..after() }
}

#[test]
fn a_stale_boot_image_is_reported() {
    // The 2026-11-17 failure: the "previous" image holding the old kernel sits in the boot menu.
    let now = zen_with(Some(vec![
        uki("arch-linux-zen", "7.3.5-zen1-1-zen"),
        uki("arch-linux-zen-previous", "7.3.4-zen2-1-zen"),
    ]));
    let r = &after_plans(&now, &now, &[], &Options::default(), false)[6];
    assert_eq!(r.todo, Todo::Report);
    assert_eq!(r.summary, "1 boot image(s) hold a kernel that is not installed");
    assert!(r.details[0].contains("arch-linux-zen-previous.efi holds kernel 7.3.4-zen2-1-zen"), "{:?}", r.details);
}

#[test]
fn a_uki_system_needs_no_initramfs_in_boot() {
    // solon64: mkinitcpio writes /boot/EFI/Linux/arch-linux-zen.efi, and /boot keeps only vmlinuz.
    let boot = Some(vec!["vmlinuz-linux-zen".to_string(), "EFI".into()]);
    let read = After { boot: boot.clone(), ..zen_with(Some(vec![uki("arch-linux-zen", "7.3.5-zen1-1-zen")])) };
    assert_eq!(after_plans(&read, &read, &[], &Options::default(), false)[6].todo, Todo::Nothing);
    // The images couldn't be read, but the presets say UKI.
    let preset = After { boot, uki_presets: true, ..zen_with(None) };
    let r = &after_plans(&preset, &preset, &[], &Options::default(), false)[6];
    assert!(!r.summary.contains("initramfs"), "{}", r.summary);
    // A UKI that holds no installed kernel is still caught.
    let none = zen_with(Some(vec![uki("arch-linux-zen", "7.3.4-zen2-1-zen")]));
    assert!(matches!(after_plans(&none, &none, &[], &Options::default(), false)[6].todo, Todo::Run(_)));
}

#[test]
fn current_boot_images_need_nothing() {
    let now = zen_with(Some(vec![uki("arch-linux-zen", "7.3.5-zen1-1-zen")]));
    assert_eq!(after_plans(&now, &now, &[], &Options::default(), false)[6].todo, Todo::Nothing);
    let not_uki = zen_with(Some(vec![]));
    assert_eq!(after_plans(&not_uki, &not_uki, &[], &Options::default(), false)[6].todo, Todo::Nothing);
    let unread = zen_with(None);
    assert_eq!(after_plans(&unread, &unread, &[], &Options::default(), false)[6].todo, Todo::Nothing);
}

#[test]
fn a_kernel_without_a_boot_image_is_rebuilt_not_rebooted_into() {
    let now = zen_with(Some(vec![uki("arch-linux-zen", "7.3.4-zen2-1-zen")]));
    assert!(
        held(&after_plans(&now, &now, &[], &Options::default(), true)[6])
            .contains("no boot image holds the installed kernel 7.3.5-zen1-1-zen")
    );
    let fix = &after_plans(&now, &now, &[], &Options::default(), false)[6];
    match &fix.todo {
        Todo::Run(a) => assert_eq!(a.last(), Some(&Action::Root(vec!["mkinitcpio".into(), "-P".into()]))),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_kernel_modules_hook_copy_is_not_an_installed_kernel() {
    let kernels = vec![
        Kernel { version: "7.3.4-zen2-1-zen".into(), pkgbase: "linux-zen".into() },
        Kernel { version: "7.3.5-zen1-1-zen".into(), pkgbase: "linux-zen".into() },
    ];
    let now = After {
        kernels,
        unowned_kernels: vec!["7.3.4-zen2-1-zen".into()],
        ukis: Some(vec![uki("arch-linux-zen", "7.3.5-zen1-1-zen"), uki("old", "7.3.4-zen2-1-zen")]),
        ..after()
    };
    assert_eq!(after_plans(&now, &now, &[], &Options::default(), false)[6].details.len(), 1, "the old image is stale");
}

#[test]
fn a_kernel_modules_hook_copy_recommends_a_restart_but_does_not_hold_it() {
    // After an upgrade the running kernel's modules are a copy; DKMS is built for the new kernel only.
    let kernels = vec![
        Kernel { version: "7.3.4-zen2-1-zen".into(), pkgbase: "linux-zen".into() },
        Kernel { version: "7.3.4-zen2-2-zen".into(), pkgbase: "linux-zen".into() },
    ];
    let dkms = vec![Dkms { module: "nvidia".into(), kernel: "7.3.4-zen2-2-zen".into(), state: "installed".into() }];
    let now = After {
        kernels,
        dkms,
        unowned_kernels: vec!["7.3.4-zen2-1-zen".into()],
        kernel_modules_present: true,
        running_kernel: "7.3.4-zen2-1-zen".into(),
        ..after()
    };
    let check = &after_plans(&now, &now, &[], &Options::default(), false)[6];
    assert!(!matches!(check.todo, Todo::Hold(_) | Todo::Run(_)), "{check:?}");
    assert_eq!(check.todo, Todo::Report, "the running kernel was replaced: a restart is recommended");
    assert!(check.summary.contains("the running kernel (7.3.4-zen2-1-zen) was replaced"), "{check:?}");
}
