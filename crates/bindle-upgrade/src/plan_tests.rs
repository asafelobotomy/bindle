use super::*;
use crate::news::NewsItem;
use crate::parse::{Update, day};
use crate::prefs::Prefs;
use crate::releases::{Newer, Source};

fn up(name: &str) -> Update {
    Update { name: name.into(), old: "1-1".into(), new: "2-1".into() }
}

fn get(plans: &[StagePlan], s: Stage) -> &StagePlan {
    plans.iter().find(|p| p.stage == s).unwrap()
}

fn shown(a: &Action) -> String {
    match a {
        Action::Root(v) => format!("root: {}", v.join(" ")),
        Action::RootTerminal(v) => format!("root on the terminal: {}", v.join(" ")),
        Action::User(v) => format!("user: {}", v.join(" ")),
        Action::Releases(r) => format!("releases: {}", r.len()),
        Action::Optional { question, action, .. } => format!("{question} {}", shown(action)),
        Action::SelfUpdate { tag, .. } => format!("self-update: {tag}"),
        Action::UqSetup => "uq setup".into(),
    }
}

fn argvs(pl: &StagePlan) -> Vec<String> {
    match &pl.todo {
        Todo::Run(actions) => actions.iter().map(shown).collect(),
        other => panic!("{:?} doesn't run: {other:?}", pl.stage),
    }
}

fn news_item(title: &str, d: i64) -> NewsItem {
    NewsItem { title: title.into(), link: "https://archlinux.org/news/x/".into(), day: d }
}

#[test]
fn an_up_to_date_system_runs_nothing() {
    let plans = plan(&Probe::default(), &Options::default());
    assert_eq!(plans.len(), Stage::ALL.len());
    assert!(plans.iter().all(|p| !p.runs()), "{plans:#?}");
    assert!(plans.iter().map(|p| p.stage).eq(Stage::ALL));
}

#[test]
fn package_upgrades_are_shown_and_asked() {
    let p = Probe { pacman: Ok(vec![up("linux"), up("firefox")]), ..Default::default() };
    let plans = plan(&p, &Options::default());
    let pac = get(&plans, Stage::Pacman);
    assert_eq!(argvs(pac), ["root: pacman -Syu --noconfirm"]);
    assert_eq!(pac.summary, "2 packages to upgrade");
    assert_eq!(pac.details[0], "linux 1-1 → 2-1");
    assert!(pac.ask.as_ref().unwrap().1);
    assert_eq!(get(&plans, Stage::Reboot).summary, "will be needed: will update linux");
}

#[test]
fn on_a_terminal_pacman_asks_its_own_questions() {
    let p = Probe { pacman: Ok(vec![up("bash")]), ..Default::default() };
    let pac = plan(&p, &Options { interactive: true, ..Default::default() })
        .into_iter()
        .find(|p| p.stage == Stage::Pacman)
        .unwrap();
    assert_eq!(argvs(&pac), ["root on the terminal: pacman -Syu"]);
    assert!(pac.ask.is_none(), "pacman's own Proceed? is enough");
}

#[test]
fn the_keyring_goes_first() {
    let p = Probe { pacman: Ok(vec![up("archlinux-keyring"), up("bash")]), ..Default::default() };
    let pac = plan(&p, &Options::default()).into_iter().find(|p| p.stage == Stage::Pacman).unwrap();
    assert_eq!(argvs(&pac)[0], "root: pacman -Sy --needed --noconfirm archlinux-keyring");
}

#[test]
fn without_checkupdates_pacman_still_runs() {
    let p = Probe { pacman: Err("pacman-contrib isn't installed".into()), ..Default::default() };
    let pac = plan(&p, &Options::default()).into_iter().find(|p| p.stage == Stage::Pacman).unwrap();
    assert!(pac.runs() && pac.summary.contains("pacman-contrib"));
}

#[test]
fn news_since_the_last_upgrade_is_shown() {
    let p = Probe {
        today: day(2026, 11, 13),
        last_upgrade: Some(day(2026, 11, 1)),
        news: Ok(vec![news_item("Old", day(2026, 10, 1)), news_item("New thing", day(2026, 11, 5))]),
        pacman: Ok(vec![up("bash")]),
        ..Default::default()
    };
    let plans = plan(&p, &Options::default());
    let n = get(&plans, Stage::News);
    assert_eq!(n.summary, "1 new post");
    assert_eq!(n.details[0], "2026-11-05  New thing");
    assert!(n.ask.is_none(), "plain news is shown, not asked about");
    assert!(get(&plans, Stage::Pacman).runs());
}

#[test]
fn manual_steps_ask_or_hold_the_upgrade() {
    let p = Probe {
        today: day(2026, 11, 13),
        news: Ok(vec![news_item("Foo update requires manual intervention", day(2026, 11, 10))]),
        pacman: Ok(vec![up("foo")]),
        aur_helper: Some("paru".into()),
        aur: Ok(vec![up("bar")]),
        ..Default::default()
    };
    let asked = plan(&p, &Options::default());
    let n = get(&asked, Stage::News);
    assert!(!n.ask.as_ref().unwrap().1, "defaults to no");
    assert_eq!(n.stops, [Stage::Snapshot, Stage::Pacman, Stage::Aur, Stage::Releases]);
    assert!(n.details[0].ends_with("⚠ manual step"));
    assert!(get(&asked, Stage::Pacman).runs());

    let unattended = plan(&p, &Options { yes: true, ..Default::default() });
    assert!(matches!(get(&unattended, Stage::Pacman).todo, Todo::Hold(_)));
    assert!(matches!(get(&unattended, Stage::Aur).todo, Todo::Hold(_)));

    let ignored = plan(&p, &Options { yes: true, ignore_news: true, ..Default::default() });
    assert!(get(&ignored, Stage::Pacman).runs());
}

#[test]
fn unreadable_news_is_reported_not_fatal() {
    let p = Probe { news: Err("timeout".into()), pacman: Ok(vec![up("a")]), ..Default::default() };
    let plans = plan(&p, &Options::default());
    assert_eq!(get(&plans, Stage::News).todo, Todo::Report);
    assert!(get(&plans, Stage::Pacman).runs());
}

#[test]
fn snapshots_only_with_snapper_and_upgrades() {
    let mut p = Probe { snapper_root: true, pacman: Ok(vec![up("a")]), ..Default::default() };
    assert!(argvs(get(&plan(&p, &Options::default()), Stage::Snapshot))[0].starts_with("root: snapper -c root create"));
    p.snap_pac = true;
    assert!(matches!(get(&plan(&p, &Options::default()), Stage::Snapshot).todo, Todo::Skip(_)));
    p.snap_pac = false;
    p.pacman = Ok(vec![]);
    assert_eq!(get(&plan(&p, &Options::default()), Stage::Snapshot).todo, Todo::Nothing);
    p.snapper_root = false;
    assert!(matches!(get(&plan(&p, &Options::default()), Stage::Snapshot).todo, Todo::Skip(_)));
}

#[test]
fn stale_mirrors_are_refreshed_before_upgrading() {
    let mut p =
        Probe { reflector: true, mirrorlist_age_days: Some(30), pacman: Ok(vec![up("a")]), ..Default::default() };
    assert_eq!(argvs(get(&plan(&p, &Options::default()), Stage::Mirrors)), ["root: systemctl start reflector.service"]);
    p.mirrorlist_age_days = Some(2);
    assert_eq!(get(&plan(&p, &Options::default()), Stage::Mirrors).summary, "mirror list is 2 days old");
    p.reflector = false;
    assert!(matches!(get(&plan(&p, &Options::default()), Stage::Mirrors).todo, Todo::Skip(_)));
}

#[test]
fn aur_uses_the_installed_helper() {
    let p = Probe { aur_helper: Some("paru".into()), aur: Ok(vec![up("kyko-bin")]), ..Default::default() };
    assert_eq!(argvs(get(&plan(&p, &Options::default()), Stage::Aur)), ["user: paru -Sua"]);
    let yes = plan(&p, &Options { yes: true, ..Default::default() });
    assert_eq!(argvs(get(&yes, Stage::Aur)), ["user: paru -Sua --noconfirm --skipreview"]);
    assert!(matches!(get(&plan(&Probe::default(), &Options::default()), Stage::Aur).todo, Todo::Skip(_)));
}

#[test]
fn the_window_leaves_the_aur_to_a_terminal() {
    let p = Probe { aur_helper: Some("paru".into()), aur: Ok(vec![up("kyko-bin")]), ..Default::default() };
    let aur = get(&plan(&p, &Options { windowed: true, ..Default::default() }), Stage::Aur).clone();
    assert_eq!(aur.todo, Todo::Report);
    assert_eq!(aur.summary, "1 AUR package to upgrade: paru needs a terminal");
}

#[test]
fn firmware_is_reported_unless_asked_for() {
    let p = Probe { firmware: Some(Ok(vec!["UEFI → 1.2".into()])), ..Default::default() };
    let report = get(&plan(&p, &Options::default()), Stage::Firmware).clone();
    assert_eq!(report.todo, Todo::Report);
    assert!(report.summary.contains("--firmware"));
    let run = plan(&p, &Options { firmware: true, yes: true, ..Default::default() });
    assert_eq!(argvs(get(&run, Stage::Firmware)), ["user: fwupdmgr update -y"]);
}

#[test]
fn flatpak_updates_run_as_the_user() {
    let p = Probe { flatpak: Some(Ok(vec!["org.mozilla.firefox".into()])), ..Default::default() };
    assert_eq!(
        argvs(get(&plan(&p, &Options::default()), Stage::Flatpak)),
        [
            "user: flatpak update -y --noninteractive",
            "Remove Flatpak runtimes no app uses any more? user: flatpak uninstall --unused -y --noninteractive"
        ]
    );
}

#[test]
fn orphans_are_asked_about_or_reported() {
    let a = After { orphans: vec!["libfoo".into()], kernel_modules_present: true, ..Default::default() };
    let asked = after_plans(&a, &a, &[], &Options::default(), false);
    assert_eq!(asked[3].ask.as_ref().unwrap(), &("Remove 1 orphaned package?".to_string(), false));
    let reported = after_plans(&a, &a, &[], &Options { yes: true, ..Default::default() }, false);
    assert_eq!(reported[3].todo, Todo::Report);
    let removed = after_plans(&a, &a, &[], &Options { yes: true, remove_orphans: true, ..Default::default() }, false);
    assert_eq!(argvs(&removed[3]), ["root: pacman -Rns --noconfirm -- libfoo"]);
}

#[test]
fn reboot_follows_the_kernel_and_drivers() {
    let gone = After { running_kernel: "6.17.1-arch1-1".into(), kernel_modules_present: false, ..Default::default() };
    let r = &after_plans(&gone, &gone, &[], &Options::default(), false)[6];
    assert_eq!(r.summary, "recommended: the running kernel (6.17.1-arch1-1) was replaced");
    let fine = After { kernel_modules_present: true, ..Default::default() };
    let r = &after_plans(&fine, &fine, &["nvidia-open".into(), "bash".into()], &Options::default(), false)[6];
    assert_eq!(r.summary, "recommended: updated nvidia-open");
    assert_eq!(after_plans(&fine, &fine, &["bash".into()], &Options::default(), false)[6].todo, Todo::Nothing);
    assert!(!needs_restart("linux-api-headers") && needs_restart("linux-zen") && needs_restart("amd-ucode"));
}

#[test]
fn pacfiles_point_at_the_recipe() {
    let a =
        After { pacfiles: vec!["/etc/pacman.conf.pacnew".into()], kernel_modules_present: true, ..Default::default() };
    let pf = &after_plans(&a, &a, &[], &Options::default(), false)[2];
    assert_eq!(pf.summary, "1 file to merge: run `bindo pacnew`");
}

#[test]
fn skip_and_only_turn_stages_off() {
    let p = Probe { pacman: Ok(vec![up("a")]), flatpak: Some(Ok(vec!["x.y".into()])), ..Default::default() };
    let skip = Options { skip: [Stage::Pacman].into(), ..Default::default() };
    let plans = plan(&p, &skip);
    assert_eq!(get(&plans, Stage::Pacman).todo, Todo::Skip("turned off (--skip)".into()));
    assert!(get(&plans, Stage::Flatpak).runs());
    let only = Options { only: [Stage::Flatpak].into(), ..Default::default() };
    let plans = plan(&p, &only);
    assert!(get(&plans, Stage::Flatpak).runs());
    assert!(plans.iter().filter(|p| p.stage != Stage::Flatpak).all(|p| matches!(p.todo, Todo::Skip(_))));
}

fn newer(name: &str, source: Source) -> Newer {
    let asset = bindle_core::catalog::ReleaseAsset {
        repo: format!("o/{name}"),
        tag: "v2".into(),
        asset: format!("{name}-2-1-x86_64.pkg.tar.zst"),
        sha256: "0".repeat(64),
        name: name.into(),
        title: String::new(),
    };
    Newer { asset, installed: "1-1".into(), source }
}

#[test]
fn pinned_apps_stay_on_the_source_they_came_from() {
    let p = Probe {
        releases: vec![newer("kyko", Source::Bindle), newer("hauler", Source::Other), newer("xzram", Source::Repo)],
        ..Default::default()
    };
    let r = get(&plan(&p, &Options::default()), Stage::Releases).clone();
    assert_eq!(r.summary, "1 newer pinned release; 2 apps kept on their own source");
    assert_eq!(r.packages, ["kyko"]);
    assert!(r.details[1].starts_with("hauler: kept on the AUR or a manual install"));
    assert!(r.details[2].contains("releases.apps.xzram = \"bindle\""));
}

#[test]
fn preferences_let_bindles_release_win() {
    let p =
        Probe { releases: vec![newer("hauler", Source::Other), newer("xzram", Source::Repo)], ..Default::default() };
    let only_kept = get(&plan(&p, &Options::default()), Stage::Releases).clone();
    assert_eq!(only_kept.todo, Todo::Report);
    let mut prefs = Prefs::default();
    prefs.merge("[releases.apps]\nxzram = \"bindle\"\n").unwrap();
    let r = get(&plan(&p, &Options { prefs: prefs.clone(), ..Default::default() }), Stage::Releases).clone();
    assert_eq!(r.packages, ["xzram"]);
    assert!(r.details[0].ends_with("replacing the copy from the Arch repos)"));
    prefs.merge("[releases]\nfollow = \"bindle\"\n").unwrap();
    let all = get(&plan(&p, &Options { prefs, ..Default::default() }), Stage::Releases).clone();
    assert_eq!(all.packages, ["hauler", "xzram"]);
}

#[test]
fn mirror_and_news_ages_come_from_the_preferences() {
    let mut prefs = Prefs::default();
    prefs.merge("mirror_max_age_days = 3\nnews_days = 5\n").unwrap();
    let o = Options { prefs, ..Default::default() };
    let p = Probe {
        reflector: true,
        mirrorlist_age_days: Some(4),
        pacman: Ok(vec![up("a")]),
        today: day(2026, 11, 13),
        news: Ok(vec![news_item("Week-old post", day(2026, 11, 6))]),
        ..Default::default()
    };
    let plans = plan(&p, &o);
    assert!(get(&plans, Stage::Mirrors).runs());
    assert_eq!(get(&plans, Stage::News).summary, "nothing new in the last 5 days");
}
