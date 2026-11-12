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
