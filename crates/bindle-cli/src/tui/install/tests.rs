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
    Disk { path: "/dev/vda".into(), bytes: 64 << 30, model: "QEMU".into(), ..Disk::default() }
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

    // Fill the form: the keyboard first (kept), then the hostname, which is
    // pre-filled, so clear and type.
    assert_eq!(w.fields[w.field].key, Key::Keyboard);
    assert_eq!(w.on_key(KeyCode::Enter), Action::None, "us is what the console has already");
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
    assert!(err.contains("at least 8"), "{err}");
    type_in(&mut w, "-enough");
    assert!(w.answers().unwrap_err().contains("disk passphrases must match"));
    w.on_key(KeyCode::Enter);
    type_in(&mut w, "short-enough");
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
    w.live = true;
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

fn wizard<'a>(facts: &'a Facts, catalog: &'a Catalog, desktops: &'a [bindle_core::desktops::Desktop]) -> Wizard<'a> {
    let mut w = Wizard::new(facts, catalog, desktops);
    w.disks = vec![disk()];
    w
}

fn set(w: &mut Wizard, key: Key, text: &str) {
    w.fields.iter_mut().find(|f| f.key == key).unwrap().value = text.into();
}

#[test]
fn ctrl_c_and_modifiers() {
    use ratatui::crossterm::event::{KeyEvent, KeyModifiers};
    let (facts, catalog) = (Facts::default(), Catalog::arch().unwrap());
    let desktops = bindle_core::desktops::all().unwrap();
    let mut w = wizard(&facts, &catalog, &desktops);
    let ctrl = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
    // Nothing typed yet: Ctrl-C quits at once.
    assert_eq!(w.on_event(ctrl('c')), Action::Quit);
    w.step = Step::You;
    w.on_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    assert!(w.typed);
    // Ctrl or Alt held: nothing is typed.
    let before = w.fields[w.field].value.clone();
    w.on_event(ctrl('u'));
    w.on_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::ALT));
    assert_eq!(w.fields[w.field].value, before);
    // Something typed: Ctrl-C asks; another key stays, y quits.
    assert_eq!(w.on_event(ctrl('c')), Action::None);
    assert!(w.quit_ask && w.message.as_deref().is_some_and(|m| m.contains("Quit")));
    assert_eq!(w.on_event(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)), Action::None);
    assert!(!w.quit_ask);
    w.on_event(ctrl('c'));
    assert_eq!(w.on_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)), Action::Quit);
    // The wipe question isn't answered by Ctrl-Y or Alt-Y.
    w.step = Step::Review;
    w.confirm = true;
    assert_eq!(w.on_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)), Action::None);
    assert!(w.confirm, "still asking");
    assert_eq!(w.on_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)), Action::Install);
}

#[test]
fn keyboard_switches_the_console_and_is_checked() {
    let (facts, catalog) = (Facts::default(), Catalog::arch().unwrap());
    let desktops = bindle_core::desktops::all().unwrap();
    let mut w = wizard(&facts, &catalog, &desktops);
    w.keymaps = Some(vec!["de-latin1".into(), "uk".into(), "us".into()]);
    w.step = Step::You;
    w.field = 0;
    w.on_key(KeyCode::Backspace);
    w.on_key(KeyCode::Backspace);
    type_in(&mut w, "gb");
    // Not a keymap: not loaded, and the page won't move on.
    assert_eq!(w.on_key(KeyCode::Down), Action::None);
    assert!(w.message.as_deref().is_some_and(|m| m.contains("isn't known")));
    w.field = 0;
    w.on_key(KeyCode::Backspace);
    w.on_key(KeyCode::Backspace);
    type_in(&mut w, "uk");
    assert_eq!(w.on_key(KeyCode::Enter), Action::Keymap("uk".into()));
    assert_eq!(w.keymap, "uk");
    // Changed again once a password is in: switched, with a warning.
    set(&mut w, Key::Password, "pw");
    w.field = 0;
    w.on_key(KeyCode::Backspace);
    w.on_key(KeyCode::Backspace);
    type_in(&mut w, "de-latin1");
    assert_eq!(w.on_key(KeyCode::Tab), Action::Keymap("de-latin1".into()));
    assert!(w.message.as_deref().is_some_and(|m| m.contains("type them again")));
}

#[test]
fn you_page_checks_before_anything_is_erased() {
    let (facts, catalog) = (Facts::default(), Catalog::arch().unwrap());
    let desktops = bindle_core::desktops::all().unwrap();
    let mut w = wizard(&facts, &catalog, &desktops);
    w.passwd = "root:x:0:0::/root:/bin/bash\nnm-openconnect:x:956:956::/:/usr/bin/nologin\n".into();
    w.keymaps = Some(vec!["us".into()]);
    w.locales = Some("en_US.UTF-8 UTF-8\nen_GB.UTF-8 UTF-8\n".into());
    for (key, text) in [(Key::Username, "sddm"), (Key::Password, "pw"), (Key::Confirm, "pw"), (Key::Timezone, "UTC")] {
        set(&mut w, key, text);
    }
    assert!(w.answers().unwrap_err().contains("system name"));
    set(&mut w, Key::Username, "nm-openconnect");
    assert!(w.answers().unwrap_err().contains("belongs to the system"));
    set(&mut w, Key::Username, "solon");
    set(&mut w, Key::Keyboard, "qwerty");
    assert!(w.answers().unwrap_err().contains("Keyboard layout"));
    set(&mut w, Key::Keyboard, "us");
    set(&mut w, Key::Language, "en_XX");
    let err = w.answers().unwrap_err();
    // (No zoneinfo on a build machine without tzdata: then the timezone stops it first.)
    assert!(err.contains("Language") || err.contains("Timezone"), "{err}");
    set(&mut w, Key::Language, "en_GB");
    if let Ok(a) = w.answers() {
        assert_eq!((a.keyboard.as_str(), a.locale.as_str()), ("us", "en_GB"));
    }
}

#[test]
fn wipe_question_names_the_disk_in_full() {
    let (facts, catalog) = (Facts::default(), Catalog::arch().unwrap());
    let desktops = bindle_core::desktops::all().unwrap();
    let mut w = wizard(&facts, &catalog, &desktops);
    w.disks[0].transport = "virtio".into();
    w.step = Step::Review;
    w.confirm = true;
    let q = w.wipe_question();
    assert_eq!(q, "Erase everything on /dev/vda · 64 GiB · QEMU · virtio and install?");
    assert!(screen(&mut w, 140, 45).contains("/dev/vda · 64 GiB · QEMU · virtio"));
    // The largest fixed disk is preselected, not the first row.
    let usb = Disk { path: "/dev/sda".into(), bytes: 128 << 30, removable: true, ..Disk::default() };
    w.set_disks(vec![usb, disk()], crate::livesys::Medium::NotLive);
    assert_eq!(w.chosen_disk().map(|d| d.path.as_str()), Some("/dev/vda"));
}

#[test]
fn review_lists_aur_packages_and_hides_graphics_without_a_desktop() {
    let (facts, catalog) = (Facts::default(), Catalog::arch().unwrap());
    let desktops = bindle_core::desktops::all().unwrap();
    let mut w = wizard(&facts, &catalog, &desktops);
    // uQ (the default desktop) is built from the AUR.
    let aur = w.aur_packages().unwrap();
    assert!(aur.iter().any(|p| p.starts_with("uq")), "{aur:?}");
    w.go(Step::Review);
    let s = screen(&mut w, 140, 45);
    assert!(s.contains("built from the AUR"), "{s}");
    assert!(w.hardware().rows.iter().any(|r| r.label == "Graphics driver"));
    w.desktop.select(Some(0));
    assert!(!w.hardware().rows.iter().any(|r| r.label == "Graphics driver"));
}
