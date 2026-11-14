//! Fixtures: an old record and this catalog → the expected groups.

use super::*;
use crate::Catalog;

fn content(packages: &[&str], files: &[(&str, &str)]) -> Content {
    Content {
        packages: packages.iter().map(|s| s.to_string()).collect(),
        files: files.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect(),
        ..Default::default()
    }
}

fn plan(modules: &[(&str, Content)]) -> Plan {
    Plan {
        modules: modules.iter().map(|(id, _)| id.to_string()).collect(),
        contents: modules.iter().map(|(id, c)| (id.to_string(), c.clone())).collect(),
        ..Default::default()
    }
}

fn facts(installed: &[&str]) -> Facts {
    Facts { installed: installed.iter().map(|s| s.to_string()).collect(), ..Default::default() }
}

fn record(version: &str, modules: &[(&str, Content)]) -> Record {
    Record {
        version: Some(version.into()),
        modules: modules.iter().map(|(id, c)| (id.to_string(), c.clone())).collect(),
        unrecorded: Default::default(),
    }
}

fn run(rec: &Record, p: &Plan, table: &Changes, installed: &[&str], files: &[(&str, &str)]) -> WhatsNew {
    run_as("0.1.9", rec, p, table, installed, files)
}

fn run_as(
    version: &str,
    rec: &Record,
    p: &Plan,
    table: &Changes,
    installed: &[&str],
    files: &[(&str, &str)],
) -> WhatsNew {
    let catalog = Catalog::arch().unwrap();
    let f = facts(installed);
    let files: BTreeMap<String, String> = files.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let current = |path: &str| files.get(path).cloned();
    let excluded: std::collections::BTreeSet<String> = ["firefox".to_string()].into();
    diff(rec, p, &catalog, table, &Now { facts: &f, current_file: &current, excluded: &excluded }, version)
}

fn kinds(w: &WhatsNew) -> Vec<(String, Kind, bool)> {
    w.items.iter().map(|i| (i.id.clone(), i.kind, i.ticked)).collect()
}

#[test]
fn the_table_is_valid_for_this_catalog() {
    let catalog = Catalog::arch().unwrap();
    let t = Changes::arch(&catalog).unwrap();
    assert!(!t.added.is_empty());
    let bad = Changes::parse("[[retired]]\nid = \"printing\"\nversion = \"0.1.9\"\nwhy = \"x\"\n").unwrap();
    assert!(bad.validate(&catalog).is_err(), "printing is still in the catalog");
    let bad = Changes::parse("[[added]]\nversion = \"0.2\"\nmodules = []\nwhy = \"x\"\n").unwrap();
    assert!(bad.validate(&catalog).is_err());
}

#[test]
fn new_updated_and_unchanged_modules() {
    let printing_old = content(&["cups"], &[]);
    let printing_new = content(&["cups", "gutenprint"], &[]);
    let zram_old =
        content(&["zram-generator"], &[("/etc/systemd/zram-generator.conf", "# Managed by Bindle\nsize = ram / 2\n")]);
    let zram_new =
        content(&["zram-generator"], &[("/etc/systemd/zram-generator.conf", "# Managed by Bindle\nsize = ram\n")]);
    let rec =
        record("0.1.8", &[("printing", printing_old), ("zram", zram_old), ("firefox", content(&["firefox"], &[]))]);
    let p = plan(&[
        ("printing", printing_new),
        ("zram", zram_new),
        ("firefox", content(&["firefox"], &[])),
        ("spellcheck", content(&["hunspell-en_us"], &[])),
    ]);
    let w = run(&rec, &p, &Changes::default(), &["cups", "zram-generator", "firefox"], &[]);
    assert_eq!(
        kinds(&w),
        [
            ("printing".into(), Kind::Updated, true),
            ("zram".into(), Kind::Updated, true),
            ("spellcheck".into(), Kind::New, true)
        ]
    );
    assert_eq!(w.items[0].changes[0].text, "+ installs gutenprint");
    let file = &w.items[1].changes[0];
    assert_eq!(file.text, "~ changes /etc/systemd/zram-generator.conf");
    assert_eq!(file.diff, ["  # Managed by Bindle", "- size = ram / 2", "+ size = ram"]);
    assert_eq!(w.since.as_deref(), Some("0.1.8"));
    assert!(w.recorded);
}

#[test]
fn a_new_module_already_in_place_is_not_news() {
    let rec = record("0.1.8", &[("firefox", content(&["firefox"], &[]))]);
    let p = plan(&[("firefox", content(&["firefox"], &[])), ("spellcheck", content(&["hunspell-en_us"], &[]))]);
    let w = run(&rec, &p, &Changes::default(), &["firefox", "hunspell-en_us"], &[]);
    assert!(w.is_empty());
}

#[test]
fn optional_modules_new_since_the_last_apply_are_unticked() {
    let table =
        Changes::parse("[[added]]\nversion = \"0.1.9\"\nmodules = [\"bash-completion\"]\nwhy = \"Tab completion.\"\n")
            .unwrap();
    let rec = record("0.1.8", &[("firefox", content(&["firefox"], &[]))]);
    let p = plan(&[("firefox", content(&["firefox"], &[]))]);
    let w = run(&rec, &p, &table, &["firefox"], &[]);
    assert_eq!(kinds(&w), [("bash-completion".into(), Kind::Optional, false)]);
    let seen = record("0.1.9", &[("firefox", content(&["firefox"], &[]))]);
    assert!(run(&seen, &p, &table, &["firefox"], &[]).is_empty(), "already offered by 0.1.9");
    // A 0.1.8 build that just set this computer up: the 0.1.9 entry is for
    // a release it isn't, so nothing opens.
    assert!(run_as("0.1.8", &rec, &p, &table, &["firefox"], &[]).is_empty(), "not released yet");
}

#[test]
fn renamed_modules_carry_their_record() {
    let table =
        Changes::parse("[[renamed]]\nfrom = \"old-zram\"\nto = \"zram\"\nversion = \"0.1.9\"\nwhy = \"x\"\n").unwrap();
    let c = content(&["zram-generator"], &[]);
    let rec = record("0.1.8", &[("old-zram", c.clone())]);
    let w = run(&rec, &plan(&[("zram", c)]), &table, &["zram-generator"], &[]);
    assert!(w.is_empty(), "neither new nor retired: {:?}", kinds(&w));
}

#[test]
fn retired_and_replaced_modules_offer_a_clean_up() {
    let old =
        Content { units: vec!["earlyoom.service".into()], ..content(&["earlyoom"], &[("/etc/default/earlyoom", "x")]) };
    let rec = record("0.1.8", &[("earlyoom", old.clone()), ("gone-module", content(&["foo"], &[]))]);
    let p = plan(&[("oomd", content(&[], &[]))]);
    let w = run(&rec, &p, &Changes::default(), &[], &[]);
    let earlyoom = w.items.iter().find(|i| i.id == "earlyoom").unwrap();
    assert_eq!((earlyoom.kind, earlyoom.ticked), (Kind::Retired, false));
    assert_eq!(earlyoom.why, "replaced by Out-of-memory killer (systemd-oomd)");
    assert_eq!(earlyoom.changes[0].text, "− removes /etc/default/earlyoom (if Bindle wrote it)");
    assert_eq!(earlyoom.changes[1].text, "− turns off earlyoom.service");
    assert!(earlyoom.changes[2].text.ends_with("pacman -Rns earlyoom"));
    let gone = w.items.iter().find(|i| i.id == "gone-module").unwrap();
    assert_eq!(gone.why, "no longer part of Bindle");
}

#[test]
fn without_a_record_the_system_is_the_baseline() {
    let p =
        plan(&[("printing", content(&["cups", "gutenprint"], &[])), ("spellcheck", content(&["hunspell-en_us"], &[]))]);
    let w = run(&Record::default(), &p, &Changes::default(), &["cups"], &[]);
    assert!(!w.recorded);
    assert_eq!(kinds(&w), [("printing".into(), Kind::Updated, true), ("spellcheck".into(), Kind::New, true)]);
    assert!(w.items[0].why.starts_with("set up before Bindle kept a record"));
    assert_eq!(w.items[0].changes[0].text, "+ installs gutenprint");
}

#[test]
fn versions_order() {
    use std::cmp::Ordering::*;
    assert_eq!(crate::version_cmp("0.1.9", "0.1.8"), Greater);
    assert_eq!(crate::version_cmp("0.1.10", "0.1.9"), Greater);
    assert_eq!(crate::version_cmp("0.2.0-rc.1", "0.2.0"), Less);
    assert_eq!(crate::version_cmp("v0.1.8", "0.1.8"), Equal);
}

#[test]
fn modules_applied_before_records_compare_with_the_system() {
    let mut rec = record("0.1.9", &[("firefox", content(&["firefox"], &[]))]);
    rec.unrecorded.insert("printing".into());
    let p = plan(&[("firefox", content(&["firefox"], &[])), ("printing", content(&["cups", "gutenprint"], &[]))]);
    let w = run(&rec, &p, &Changes::default(), &["firefox", "cups"], &[]);
    assert_eq!(kinds(&w), [("printing".into(), Kind::Updated, true)]);
}

#[test]
fn old_apply_labels_decide_what_was_set_up() {
    let mut rec = Record::default();
    rec.unrecorded.insert("aur-helper".into());
    let p = plan(&[
        ("aur-helper", content(&["paru-bin", "rebuild-detector"], &[])),
        ("printing", content(&["avahi", "cups"], &[])),
    ]);
    let w = run(&rec, &p, &Changes::default(), &["paru-bin", "avahi"], &[]);
    assert_eq!(kinds(&w), [("aur-helper".into(), Kind::Updated, true), ("printing".into(), Kind::New, true)]);
}

#[test]
fn modules_the_user_took_out_are_not_called_retired() {
    let rec = record("0.1.8", &[("firefox", content(&["firefox"], &[])), ("printing", content(&["cups"], &[]))]);
    let w = run(&rec, &plan(&[]), &Changes::default(), &["firefox", "cups"], &[]);
    let kind = |id: &str| w.items.iter().find(|i| i.id == id).unwrap().kind;
    assert_eq!(kind("firefox"), Kind::Removed, "excluded in the saved selection");
    assert_eq!(kind("printing"), Kind::Retired);
    assert!(w.items.iter().all(|i| !i.ticked));
}

#[test]
fn set_up_lists_modules_whose_content_is_all_here() {
    let catalog = Catalog::arch().unwrap();
    let none = |_: &str| None;
    // bash-completion is one package; nothing installed: not set up.
    assert!(!set_up(&catalog, &facts(&[]), &none).contains("bash-completion"));
    let here = set_up(&catalog, &facts(&["bash-completion", "exfatprogs", "dosfstools"]), &none);
    assert!(here.contains("bash-completion"));
    assert!(!here.contains("disk-formats"), "ntfs-3g is still missing");
    // A file module counts only when the file is exactly Bindle's.
    let m = catalog.module("makepkg-parallel").unwrap();
    let (path, text) = (m.files[0].path.clone(), m.files[0].content.clone());
    let exact = move |p: &str| (p == path).then(|| text.clone());
    assert!(set_up(&catalog, &facts(&[]), &exact).contains("makepkg-parallel"));
    // The AUR helper: any installed helper.
    assert!(!set_up(&catalog, &facts(&[]), &none).contains("aur-helper"));
    let paru = Facts { aur_helper: Some("paru".into()), ..facts(&[]) };
    assert!(set_up(&catalog, &paru, &none).contains("aur-helper"));
}
