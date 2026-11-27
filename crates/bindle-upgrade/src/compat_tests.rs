use super::compat::*;
use super::compat_probe::{candidates, info_url, installed};
use super::parse::Update;
use super::vercmp::Dep;

fn pkg(version: &str, conflicts: &[&str], depends: &[&str]) -> Pkg {
    let d = |l: &[&str]| l.iter().map(|x| Dep::parse(x).unwrap()).collect();
    Pkg { version: version.into(), conflicts: d(conflicts), depends: d(depends), provides: vec![] }
}

fn up(name: &str, old: &str, new: &str) -> Update {
    Update { name: name.into(), old: old.into(), new: new.into() }
}

fn facts(installed: &[(&str, Pkg)], candidates: &[(&str, Pkg)]) -> Facts {
    let m = |l: &[(&str, Pkg)]| l.iter().map(|(n, p)| (n.to_string(), p.clone())).collect();
    Facts { installed: m(installed), candidates: m(candidates), problem: None }
}

fn held(v: &Verdict) -> Vec<&str> {
    v.held.iter().map(|h| h.name.as_str()).collect()
}

#[test]
fn nothing_waits_when_every_partner_is_new_enough() {
    let f = facts(
        &[("uq", pkg("0.3.42-1", &[], &[])), ("hauler", pkg("0.2.5-1", &[], &[]))],
        &[("hauler", pkg("0.2.6-1", &["uq<0.3.41"], &[]))],
    );
    let v = check(&f, &[up("hauler", "0.2.5-1", "0.2.6-1")]);
    assert_eq!(v, Verdict::default());
}

#[test]
fn an_app_waits_for_a_partner_that_isnt_on_the_aur_yet() {
    // hauler 0.2.6 needs oysterAV 0.6; the AUR still has 0.5.0.
    let f = facts(
        &[
            ("oysterav", pkg("0.5.0-1", &[], &[])),
            ("hauler", pkg("0.2.5-1", &[], &[])),
            ("kyko", pkg("1.3.2-1", &[], &[])),
        ],
        &[("hauler", pkg("0.2.6-1", &["oysterav<0.6.0"], &[])), ("kyko", pkg("1.3.3-1", &[], &[]))],
    );
    let v = check(&f, &[up("hauler", "0.2.5-1", "0.2.6-1"), up("kyko", "1.3.2-1", "1.3.3-1")]);
    assert_eq!(held(&v), ["hauler"]);
    assert!(v.held[0].reason.contains("waits for oysterav 0.6.0"), "{}", v.held[0].reason);
    assert_eq!(v.waves, None);
}

#[test]
fn an_update_that_would_break_an_app_waits_for_the_app() {
    // uQ 0.4 drops what hauler 0.2.5 uses and says so; hauler has no update yet.
    let f = facts(
        &[("uq", pkg("0.3.42-1", &[], &[])), ("hauler", pkg("0.2.5-1", &[], &[]))],
        &[("uq", pkg("0.4.0-1", &["hauler<0.3.0"], &[]))],
    );
    let v = check(&f, &[up("uq", "0.3.42-1", "0.4.0-1")]);
    assert_eq!(held(&v), ["uq"]);
    // ...and an installed app that says it can't take the new version holds it too.
    let f = facts(
        &[("uq", pkg("0.3.42-1", &[], &[])), ("hauler", pkg("0.2.5-1", &["uq>=0.4"], &[]))],
        &[("uq", pkg("0.4.0-1", &[], &[]))],
    );
    let v = check(&f, &[up("uq", "0.3.42-1", "0.4.0-1")]);
    assert_eq!(held(&v), ["uq"]);
    assert!(v.held[0].reason.contains("would break hauler 0.2.5-1"), "{}", v.held[0].reason);
}

#[test]
fn holding_one_back_holds_what_needs_it() {
    // romforge waits for oysterAV; reelo's new version needs that romforge.
    let f = facts(
        &[
            ("oysterav", pkg("0.5.0-1", &[], &[])),
            ("romforge-bin", pkg("0.22.2-1", &[], &[])),
            ("reelo", pkg("0.1.5-1", &[], &[])),
        ],
        &[
            ("romforge-bin", pkg("0.23.0-1", &["oysterav<0.6"], &[])),
            ("reelo", pkg("0.1.6-1", &["romforge-bin<0.23"], &[])),
        ],
    );
    let v = check(&f, &[up("romforge-bin", "0.22.2-1", "0.23.0-1"), up("reelo", "0.1.5-1", "0.1.6-1")]);
    assert_eq!(held(&v), ["romforge-bin", "reelo"]);
}

#[test]
fn partners_updating_together_go_in_order() {
    // hauler 0.2.6 needs uQ 0.3.43, which is in the same run: uQ first.
    let f = facts(
        &[("uq", pkg("0.3.42-1", &[], &[])), ("hauler", pkg("0.2.5-1", &[], &[])), ("kyko", pkg("1.3.2-1", &[], &[]))],
        &[
            ("uq", pkg("0.3.43-1", &[], &[])),
            ("hauler", pkg("0.2.6-1", &["uq<0.3.43"], &[])),
            ("kyko", pkg("1.3.3-1", &[], &[])),
        ],
    );
    let ups = [up("uq", "0.3.42-1", "0.3.43-1"), up("hauler", "0.2.5-1", "0.2.6-1"), up("kyko", "1.3.2-1", "1.3.3-1")];
    let v = check(&f, &ups);
    assert!(v.held.is_empty());
    let waves = v.waves.unwrap();
    let names: Vec<Vec<&str>> = waves.iter().map(|w| w.names.iter().map(String::as_str).collect()).collect();
    assert_eq!(names, [vec!["kyko", "uq"], vec!["hauler"]]);
    assert!(waves.iter().all(|w| !w.together));
}

#[test]
fn partners_that_need_each_other_go_in_one_transaction() {
    // uQ 0.4 drops hauler < 0.3; hauler 0.3 needs uQ 0.4: neither can go first.
    let f = facts(
        &[("uq", pkg("0.3.42-1", &[], &[])), ("hauler", pkg("0.2.5-1", &[], &[]))],
        &[("uq", pkg("0.4.0-1", &["hauler<0.3.0"], &[])), ("hauler", pkg("0.3.0-1", &["uq<0.4.0"], &[]))],
    );
    let v = check(&f, &[up("uq", "0.3.42-1", "0.4.0-1"), up("hauler", "0.2.5-1", "0.3.0-1")]);
    assert!(v.held.is_empty());
    let waves = v.waves.unwrap();
    assert_eq!(waves.len(), 1);
    assert!(waves[0].together);
    assert_eq!(waves[0].names, ["hauler", "uq"]);
}

#[test]
fn a_versioned_depends_on_an_aur_package_is_checked() {
    // xzram-gui 0.7.7 needs xzram 0.7.7, but only the GUI has an update.
    let f = facts(
        &[("xzram", pkg("0.7.6-1", &[], &[])), ("xzram-gui", pkg("0.7.6-1", &[], &["xzram=0.7.6"]))],
        &[("xzram-gui", pkg("0.7.7-1", &[], &["xzram=0.7.7", "qt6-base>=6.8"]))],
    );
    let v = check(&f, &[up("xzram-gui", "0.7.6-1", "0.7.7-1")]);
    assert_eq!(held(&v), ["xzram-gui"]);
    assert!(v.held[0].reason.contains("needs xzram=0.7.7"), "{}", v.held[0].reason);
}

#[test]
fn unversioned_conflicts_and_unknown_updates_are_left_to_pacman() {
    // romforge vs romforge-bin swap is pacman's question; an update the AUR
    // said nothing about still goes.
    let f = facts(
        &[("romforge-bin", pkg("0.22.2-1", &["romforge"], &[]))],
        &[("romforge", pkg("0.23.0-1", &["romforge-bin"], &[]))],
    );
    let v = check(&f, &[up("romforge-bin", "0.22.2-1", "0.23.0-1"), up("mystery", "1-1", "2-1")]);
    assert_eq!(v, Verdict::default());
}

#[test]
fn reads_pacman_qi() {
    let text = "Name            : hauler\nVersion         : 0.2.5-1\nDepends On      : glibc  qt6-base\n                  uq>=0.3.41\nProvides        : None\nConflicts With  : oysterav<0.5.0  uq<0.3.41\n\nName            : crum-bs-bin\nVersion         : 0.8.4-1\nProvides        : crum-bs=0.8.4\nConflicts With  : crum-bs  crumbs\n";
    let m = installed(text);
    let h = &m["hauler"];
    assert_eq!(h.version, "0.2.5-1");
    assert_eq!(h.conflicts.iter().map(Dep::text).collect::<Vec<_>>(), ["oysterav<0.5.0", "uq<0.3.41"]);
    assert_eq!(h.depends.last().unwrap().text(), "uq>=0.3.41");
    assert_eq!(m["crum-bs-bin"].provides[0].text(), "crum-bs=0.8.4");
}

#[test]
fn reads_the_aur_info_api() {
    let json = r#"{"resultcount":1,"results":[{"Name":"hauler","Version":"0.2.6-1","Conflicts":["oysterav<0.6.0"],"Depends":["glibc"]}],"type":"multiinfo","version":5}"#;
    let c = candidates(json).unwrap();
    assert_eq!(c["hauler"].conflicts[0].text(), "oysterav<0.6.0");
    assert!(candidates(r#"{"type":"error","error":"Too many package names."}"#).unwrap_err().contains("Too many"));
    assert_eq!(
        info_url(&["uq", "bad name", "crum-bs-bin"]),
        "https://aur.archlinux.org/rpc/v5/info?arg[]=uq&arg[]=crum-bs-bin"
    );
}

mod in_the_plan {
    use super::*;
    use crate::plan::{Action, Options, Todo, plan};
    use crate::probe::Probe;
    use crate::stage::Stage;

    fn aur_runs(p: &Probe, o: &Options) -> (Vec<String>, crate::plan::StagePlan) {
        let pl = plan(p, o).into_iter().find(|pl| pl.stage == Stage::Aur).unwrap();
        let runs = match &pl.todo {
            Todo::Run(a) => a
                .iter()
                .map(|a| match a {
                    Action::UserElevated(v) => v.join(" "),
                    other => panic!("{other:?}"),
                })
                .collect(),
            _ => vec![],
        };
        (runs, pl)
    }

    #[test]
    fn a_held_update_is_left_out_and_said() {
        let aur_compat = facts(
            &[("oysterav", pkg("0.5.0-1", &[], &[])), ("hauler", pkg("0.2.5-1", &[], &[]))],
            &[("hauler", pkg("0.2.6-1", &["oysterav<0.6.0"], &[])), ("kyko", pkg("1.3.3-1", &[], &[]))],
        );
        let aur = Ok(vec![up("hauler", "0.2.5-1", "0.2.6-1"), up("kyko", "1.3.2-1", "1.3.3-1")]);
        let p = Probe { aur_helper: Some("paru".into()), aur, aur_compat, ..Default::default() };
        let (runs, pl) = aur_runs(&p, &Options { yes: true, ..Default::default() });
        assert_eq!(runs, ["paru -Sua --noconfirm --skipreview --ignore hauler"]);
        assert_eq!(pl.packages, ["kyko"]);
        assert!(pl.summary.ends_with("1 held back until a partner is ready"), "{}", pl.summary);
        assert!(pl.details.iter().any(|d| d.starts_with("held back: hauler 0.2.6-1")), "{:?}", pl.details);
    }

    #[test]
    fn only_held_updates_is_a_report() {
        let aur_compat = facts(
            &[("uq", pkg("0.3.42-1", &[], &[])), ("hauler", pkg("0.2.5-1", &[], &[]))],
            &[("uq", pkg("0.4.0-1", &["hauler<0.3.0"], &[]))],
        );
        let aur = Ok(vec![up("uq", "0.3.42-1", "0.4.0-1")]);
        let p = Probe { aur_helper: Some("paru".into()), aur, aur_compat, ..Default::default() };
        let (runs, pl) = aur_runs(&p, &Options::default());
        assert!(runs.is_empty());
        assert_eq!(pl.todo, Todo::Report);
    }

    #[test]
    fn partners_go_first_in_their_own_run() {
        let aur_compat = facts(
            &[("uq", pkg("0.3.42-1", &[], &[])), ("hauler", pkg("0.2.5-1", &[], &[]))],
            &[("uq", pkg("0.3.43-1", &[], &[])), ("hauler", pkg("0.2.6-1", &["uq<0.3.43"], &[]))],
        );
        let aur = Ok(vec![up("hauler", "0.2.5-1", "0.2.6-1"), up("uq", "0.3.42-1", "0.3.43-1")]);
        let p = Probe { aur_helper: Some("yay".into()), aur, aur_compat, ..Default::default() };
        let (runs, _) = aur_runs(&p, &Options::default());
        assert_eq!(runs, ["yay -S --needed -- uq", "yay -S --needed -- hauler"]);
    }

    #[test]
    fn an_unchecked_aur_still_updates_and_says_so() {
        let aur_compat = Facts { problem: Some("curl failed".into()), ..Default::default() };
        let p = Probe {
            aur_helper: Some("paru".into()),
            aur: Ok(vec![up("kyko", "1-1", "2-1")]),
            aur_compat,
            ..Default::default()
        };
        let (runs, pl) = aur_runs(&p, &Options::default());
        assert_eq!(runs, ["paru -Sua"]);
        assert!(pl.details.iter().any(|d| d.contains("couldn't check partner versions (curl failed)")));
    }
}
