use super::*;
use crate::parse::Update;
use crate::plan::{Action, Options, after_plans, plan};
use crate::probe::{After, Probe};

#[derive(Default)]
struct Script {
    answers: Vec<bool>,
    lines: Vec<String>,
    asked: Vec<String>,
}

impl Ui for Script {
    fn stage(&mut self, _: usize, _: usize, _: &StagePlan) {}
    fn line(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }
    fn ask(&mut self, q: &str, _: bool) -> bool {
        self.asked.push(q.to_string());
        if self.answers.is_empty() { true } else { self.answers.remove(0) }
    }
}

fn up(name: &str) -> Update {
    Update { name: name.into(), old: "1".into(), new: "2".into() }
}

fn dry(yes: bool) -> RunOpts {
    RunOpts { yes, dry_run: true, ..Default::default() }
}

fn outcome(results: &[StageResult], s: Stage) -> Outcome {
    results.iter().find(|r| r.stage == s).unwrap().outcome.clone()
}

#[test]
fn a_dry_run_prints_commands_and_reports_each_stage() {
    let p = Probe { pacman: Ok(vec![up("linux")]), flatpak: Some(Ok(vec!["org.x.Y".into()])), ..Default::default() };
    let o = Options::default();
    let mut ui = Script::default();
    let mut seen = vec![];
    let results = run(
        plan(&p, &o),
        &mut |u, _| {
            seen = u.to_vec();
            after_plans(&After::default(), &After { kernel_modules_present: true, ..Default::default() }, u, &o, false)
        },
        &dry(false),
        &mut ui,
    );
    assert_eq!(results.len(), Stage::ALL.len());
    assert_eq!(outcome(&results, Stage::Pacman), Outcome::Done);
    assert_eq!(outcome(&results, Stage::Flatpak), Outcome::Done);
    assert!(ui.lines.contains(&"would run as root: pacman -Syu --noconfirm".to_string()));
    assert!(ui.lines.contains(&"would run: flatpak update -y --noninteractive".to_string()));
    assert_eq!(seen, ["linux"], "the closing stages hear what was upgraded");
    assert_eq!(outcome(&results, Stage::Reboot), Outcome::Reported);
}

#[test]
fn saying_no_to_the_news_holds_the_upgrades() {
    let p = Probe {
        news: Ok(vec![crate::news::NewsItem {
            title: "X requires manual intervention".into(),
            link: String::new(),
            day: 0,
        }]),
        pacman: Ok(vec![up("x")]),
        ..Default::default()
    };
    let mut ui = Script { answers: vec![false], ..Default::default() };
    let results = run(plan(&p, &Options::default()), &mut |_, _| vec![], &dry(false), &mut ui);
    assert_eq!(outcome(&results, Stage::News), Outcome::Reported);
    assert!(matches!(outcome(&results, Stage::Pacman), Outcome::Held(_)));
    assert_eq!(ui.asked.len(), 1, "the upgrade question isn't asked after stopping");
}

#[test]
fn yes_answers_every_question() {
    let p = Probe { pacman: Ok(vec![up("x")]), ..Default::default() };
    let mut ui = Script::default();
    let results = run(plan(&p, &Options::default()), &mut |_, _| vec![], &dry(true), &mut ui);
    assert!(ui.asked.is_empty());
    assert_eq!(outcome(&results, Stage::Pacman), Outcome::Done);
}

#[test]
fn declining_a_stage_skips_it_and_what_needs_it() {
    let p = Probe {
        pacman: Ok(vec![up("x")]),
        aur_helper: Some("paru".into()),
        aur: Ok(vec![up("y")]),
        ..Default::default()
    };
    let mut ui = Script { answers: vec![false], ..Default::default() };
    let results = run(plan(&p, &Options::default()), &mut |_, _| vec![], &dry(false), &mut ui);
    assert_eq!(outcome(&results, Stage::Pacman), Outcome::Skipped("you said no".into()));
    assert!(matches!(outcome(&results, Stage::Aur), Outcome::Held(_)));
}

#[test]
fn a_failed_user_stage_doesnt_stop_the_run() {
    let failing = StagePlan {
        stage: Stage::Flatpak,
        todo: Todo::Run(vec![Action::User(vec!["false".into()])]),
        summary: "s".into(),
        details: vec![],
        ask: None,
        stops: vec![],
        packages: vec![],
    };
    let mut firmware = failing.clone();
    firmware.stage = Stage::Firmware;
    firmware.todo = Todo::Run(vec![Action::User(vec!["true".into()])]);
    let mut ui = Script::default();
    let opts = RunOpts::default();
    let results = run(vec![failing, firmware], &mut |_, _| vec![], &opts, &mut ui);
    assert!(matches!(&results[0].outcome, Outcome::Failed(m) if m.contains("`false` failed")));
    assert_eq!(results[1].outcome, Outcome::Done);
}

#[test]
fn a_failed_system_upgrade_holds_the_aur() {
    let mut ex_ui = Script::default();
    let opts = dry(true);
    let mut ex = Exec {
        opts: &opts,
        ui: &mut ex_ui,
        helper: None,
        refused: None,
        log: String::new(),
        blocked: BTreeMap::new(),
        updated: vec![],
    };
    ex.failed(Stage::Pacman);
    assert!(ex.blocked.contains_key(&Stage::Aur) && ex.blocked.contains_key(&Stage::Releases));
}

#[test]
fn a_refused_password_isnt_asked_for_again() {
    let mut ui = Script::default();
    let opts = RunOpts::default();
    let mut ex = Exec {
        opts: &opts,
        ui: &mut ui,
        helper: None,
        refused: Some("couldn't get administrator rights: dismissed".into()),
        log: String::new(),
        blocked: BTreeMap::new(),
        updated: vec![],
    };
    let e = ex.action(&Action::Root(vec!["true".into()])).unwrap_err();
    assert_eq!(e.to_string(), "couldn't get administrator rights: dismissed");
    assert!(ui.lines.is_empty(), "no second \"Asking for administrator rights\"");
}

#[test]
fn a_self_update_hands_the_rest_to_the_new_bindle() {
    let done = |stage| StageResult { stage, outcome: Outcome::Done, summary: String::new() };
    assert!(relaunch(&[done(Stage::Bindle)], false));
    assert!(!relaunch(&[done(Stage::Bindle)], true), "a rehearsal changed nothing");
    assert!(!relaunch(&[done(Stage::Pacman)], false));
}

#[test]
fn optional_clean_ups_follow_their_answer() {
    let clean = Action::Optional {
        question: "Remove unused runtimes?".into(),
        default: true,
        action: Box::new(Action::User(vec!["flatpak".into(), "uninstall".into(), "--unused".into()])),
    };
    let pl = StagePlan {
        stage: Stage::Flatpak,
        todo: Todo::Run(vec![clean]),
        summary: "s".into(),
        details: vec![],
        ask: None,
        stops: vec![],
        packages: vec![],
    };
    let mut ui = Script { answers: vec![false], ..Default::default() };
    run(vec![pl.clone()], &mut |_, _| vec![], &dry(false), &mut ui);
    assert_eq!(ui.asked, ["Remove unused runtimes?"]);
    assert!(ui.lines.is_empty(), "declined: nothing would run");
    let mut ui = Script::default();
    run(vec![pl], &mut |_, _| vec![], &dry(false), &mut ui);
    assert_eq!(ui.lines, ["would run: flatpak uninstall --unused"]);
}
