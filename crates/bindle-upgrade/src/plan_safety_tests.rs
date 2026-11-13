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
            Rebuild { repo: "foreign".into(), name: "python-foo".into() },
            Rebuild { repo: "extra".into(), name: "bar".into() },
        ]),
        ..after()
    };
    let r = &after_plans(&now, &now, &["python".into()], &Options::default(), false)[0];
    assert!(matches!(&r.todo, Todo::Run(a) if a == &[Action::User(
        ["paru", "-S", "--rebuild", "--", "python-foo"].map(String::from).to_vec()
    )]));
    assert!(r.details[1].contains("bar (from extra"));
    let w = &after_plans(&now, &now, &[], &Options { windowed: true, ..Default::default() }, false)[0];
    assert_eq!(w.todo, Todo::Report);
    let missing = &after_plans(&after(), &after(), &["python".into()], &Options::default(), false)[0];
    assert!(missing.summary.starts_with("install rebuild-detector"));
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
    assert!(fix.ask.as_ref().unwrap().1, "asked, default yes");
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
