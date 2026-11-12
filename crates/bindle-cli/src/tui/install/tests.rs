use bindle_core::{Catalog, Facts};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;

use super::*;

fn type_in(w: &mut Wizard, s: &str) {
    s.chars().for_each(|c| drop(w.on_key(KeyCode::Char(c))));
}

fn screen(w: &mut Wizard, width: u16, height: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| w.draw(f)).unwrap();
    term.backend().buffer().content().iter().map(|c| c.symbol()).collect()
}

fn disk() -> Disk {
    Disk { path: "/dev/vda".into(), bytes: 64 << 30, model: "QEMU".into(), transport: String::new() }
}

#[test]
fn walks_every_step_to_install() {
    let (facts, catalog) = (Facts::default(), Catalog::arch().unwrap());
    let desktops = bindle_core::desktops::all().unwrap();
    let mut w = Wizard::new(&facts, &catalog, &desktops);
    w.disks = vec![disk()];

    // Offline: Network won't move on.
    assert_eq!(w.on_key(KeyCode::Enter), Action::None);
    assert_eq!(w.step, Step::Network);
    assert!(w.message.is_some());
    w.online = true;
    w.on_key(KeyCode::Enter);
    assert_eq!(w.step, Step::Desktop);
    // uQ (recommended) is pre-selected, so its apps come along.
    assert_eq!(w.choice().desktop.as_deref(), Some("uq"));
    assert!(w.selection().include.contains(&"uq".to_string()));
    w.on_key(KeyCode::Enter);
    assert_eq!(w.step, Step::Hardware);
    // Second row down from the top is changeable: Right changes it, r undoes.
    assert!(w.hw.is_none());
    w.on_key(KeyCode::Down);
    w.on_key(KeyCode::Down);
    w.on_key(KeyCode::Right);
    assert!(w.hardware().changed());
    w.on_key(KeyCode::Char('r'));
    assert!(!w.hardware().changed());
    w.on_key(KeyCode::Enter);
    assert_eq!(w.step, Step::Disk);
    w.on_key(KeyCode::Char('e'));
    w.on_key(KeyCode::Enter);
    assert_eq!(w.step, Step::You);

    // Fill the form: hostname is pre-filled, so clear and type.
    for _ in 0..20 {
        w.on_key(KeyCode::Backspace);
    }
    type_in(&mut w, "laptop");
    w.on_key(KeyCode::Enter);
    type_in(&mut w, "solon");
    w.on_key(KeyCode::Enter);
    type_in(&mut w, "pw");
    w.on_key(KeyCode::Enter);
    type_in(&mut w, "pw");
    w.on_key(KeyCode::Enter);
    type_in(&mut w, "short");
    let err = w.answers().unwrap_err();
    assert!(err.contains("passphrase") || err.contains("Timezone"), "{err}");
    type_in(&mut w, "-enough");
    assert_eq!(w.answers().map(|a| a.encryption), Ok(Some("short-enough".to_string())).map_err(|e: String| e));

    // Apps: every app of the profile is ticked; Space unticks the highlighted one.
    w.step = Step::Apps;
    let first = w.app_modules()[0].id.clone();
    assert!(w.selection().initial(&catalog).unwrap().contains(&first));
    w.on_key(KeyCode::Char(' '));
    assert!(!w.selection().initial(&catalog).unwrap().contains(&first));
    assert_eq!(w.app_modules()[0].id, first, "an unticked app stays listed");
    w.on_key(KeyCode::Char(' '));
    assert!(w.selection().initial(&catalog).unwrap().contains(&first));
    assert_eq!(w.on_key(KeyCode::Char('m')), Action::Apps);
    w.on_key(KeyCode::Enter);
    assert_eq!(w.step, Step::Review);
}

#[test]
fn hardware_changes_reach_the_answers_and_reset_with_the_desktop() {
    let (facts, catalog) = (Facts::default(), Catalog::arch().unwrap());
    let desktops = bindle_core::desktops::all().unwrap();
    let mut w = Wizard::new(&facts, &catalog, &desktops);
    w.step = Step::Hardware;
    let kernel = w.hardware().rows.iter().position(|r| r.label == "Kernel").unwrap();
    w.hw_row.select(Some(kernel));
    w.on_key(KeyCode::Left);
    assert_eq!(w.hardware().rows[kernel].choice().value, "linux-zen");
    // Left changes the row; it doesn't go back a page.
    assert_eq!(w.step, Step::Hardware);
    w.on_key(KeyCode::Esc);
    assert_eq!(w.step, Step::Desktop);
    assert!(w.hw.is_some(), "going back keeps the changes");
    w.on_key(KeyCode::Down);
    assert!(w.hw.is_none(), "another desktop starts from its own recommendation");
}

#[test]
fn passphrase_box_and_quit() {
    let (facts, catalog) = (Facts::default(), Catalog::arch().unwrap());
    let desktops = bindle_core::desktops::all().unwrap();
    let mut w = Wizard::new(&facts, &catalog, &desktops);
    w.networks = vec![WifiNetwork { ssid: "Home".into(), security: "psk".into(), signal: 3, connected: false }];
    w.on_key(KeyCode::Char('c'));
    assert!(w.passphrase.is_some());
    type_in(&mut w, "secret q");
    assert_eq!(w.on_key(KeyCode::Enter), Action::Connect("Home".into(), Some("secret q".into())));
    assert_eq!(w.on_key(KeyCode::Char('q')), Action::Quit);
}

#[test]
fn every_step_renders_on_an_80_column_console() {
    let (facts, catalog) = (Facts::default(), Catalog::arch().unwrap());
    let desktops = bindle_core::desktops::all().unwrap();
    let mut w = Wizard::new(&facts, &catalog, &desktops);
    w.disks = vec![disk()];
    w.networks = vec![WifiNetwork { ssid: "Home".into(), security: "psk".into(), signal: 3, connected: true }];
    w.wifi = Some("wlan0".into());
    let expect = [
        (Step::Network, "Wi-Fi networks"),
        (Step::Desktop, "What gets installed"),
        (Step::Hardware, "Recommended for this hardware"),
        (Step::Apps, "will be installed"),
        (Step::Disk, "will be erased"),
        (Step::You, "Computer name"),
        (Step::Review, "nothing changes until"),
    ];
    for (step, text) in expect {
        w.step = step;
        for (wd, ht) in [(80, 25), (140, 45)] {
            let s = screen(&mut w, wd, ht);
            assert!(s.contains(text) && s.contains("Bindle install"), "{step:?} {wd}x{ht}");
        }
    }
}

#[test]
fn manual_layout_replaces_the_disk_choice() {
    let (facts, catalog) = (Facts::default(), Catalog::arch().unwrap());
    let desktops = bindle_core::desktops::all().unwrap();
    let mut w = Wizard::new(&facts, &catalog, &desktops);
    w.step = Step::Disk;
    assert_eq!(w.on_key(KeyCode::Char('p')), Action::Partition);
    // No disk Bindle could use: with a manual layout the page still moves on.
    w.on_key(KeyCode::Enter);
    assert_eq!(w.step, Step::Disk);
    let saved = r#"{"disk_config":{"config_type":"manual_partitioning","device_modifications":[
        {"device":"/dev/sdb","wipe":false,"partitions":[{"obj_id":"a"}]}]},"encryption_password":"from-editor"}"#;
    w.manual = manual::ManualDisk::parse(saved);
    w.encrypt = true;
    assert!(!w.encrypting(), "the editor's own encryption applies, not Bindle's");
    assert!(screen(&mut w, 140, 45).contains("Manual layout"));
    w.on_key(KeyCode::Char('e'));
    assert!(w.message.is_some() && w.encrypt, "f and e don't apply to a manual layout");
    w.on_key(KeyCode::Enter);
    assert_eq!(w.step, Step::You);
    for (key, text) in [(Key::Username, "solon"), (Key::Password, "pw"), (Key::Confirm, "pw")] {
        w.fields.iter_mut().find(|f| f.key == key).unwrap().value = text.into();
    }
    w.fields.iter_mut().find(|f| f.key == Key::Timezone).unwrap().value = "UTC".into();
    if let Ok(a) = w.answers() {
        // (No zoneinfo on a build machine without tzdata: then answers() stops at the timezone.)
        assert_eq!(a.disk, "/dev/sdb");
        assert_eq!(a.encryption.as_deref(), Some("from-editor"));
        assert_eq!(a.manual_disk.unwrap()["config_type"], "manual_partitioning");
    }
    w.step = Step::Disk;
    w.on_key(KeyCode::Char('x'));
    assert!(w.manual.is_none());
}
