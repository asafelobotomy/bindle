//! The event loop of `bindle install`: keys go to the wizard; scanning,
//! joining Wi-Fi, the disk editor, loadkeys and the apps screen happen
//! here. Off the live ISO (a dry run) Wi-Fi, the editor and loadkeys aren't
//! used: they would change the machine the dry run is on.

use anyhow::Result;
use bindle_core::desktops::Desktop;
use bindle_core::{Catalog, Facts};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use super::{Action, Key, Outcome, Wizard};
use crate::livesys;

fn refresh_network(w: &mut Wizard) {
    w.online = crate::iso::online();
    w.wifi = livesys::wifi_device();
    if !w.online
        && let Some(dev) = w.wifi.clone()
    {
        w.networks = livesys::scan(&dev).unwrap_or_default();
    }
}

/// Run the wizard. None: the user quit. `live`: on the live ISO.
pub fn run(facts: &Facts, catalog: &Catalog, desktops: &[Desktop], live: bool) -> Result<Option<Outcome>> {
    if catalog.profiles.is_empty() {
        anyhow::bail!("the catalog has no profiles");
    }
    let mut w = Wizard::new(facts, catalog, desktops);
    let (disks, medium) = livesys::disks();
    w.set_disks(disks, medium);
    w.uefi = livesys::uefi();
    w.live = live;
    w.keymaps = livesys::keymaps();
    w.locales = livesys::supported_locales();
    w.passwd = livesys::system_users();
    w.fields.iter_mut().find(|f| f.key == Key::Timezone).expect("timezone field").value = livesys::default_timezone();
    refresh_network(&mut w);
    let mut terminal = ratatui::init();
    let result = event_loop(&mut w, &mut terminal);
    ratatui::restore();
    result
}

fn event_loop(w: &mut Wizard, terminal: &mut DefaultTerminal) -> Result<Option<Outcome>> {
    loop {
        terminal.draw(|f| w.draw(f))?;
        let Event::Key(k) = event::read()? else { continue };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        match w.on_event(k) {
            Action::None => {}
            Action::Quit => return Ok(None),
            Action::Scan => {
                w.message = Some("Looking for networks…".into());
                terminal.draw(|f| w.draw(f))?;
                refresh_network(w);
                w.message = None;
            }
            Action::Connect(..) if !w.live => {
                w.message = Some("Not on the live ISO (a dry run): Wi-Fi isn't joined here.".into());
            }
            Action::Connect(ssid, pass) => {
                w.message = Some(format!("Joining {ssid}…"));
                terminal.draw(|f| w.draw(f))?;
                let dev = w.wifi.clone().unwrap_or_default();
                w.message = match livesys::connect(&dev, &ssid, pass.as_deref()) {
                    Ok(()) => {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        refresh_network(w);
                        Some(if w.online {
                            format!("Connected to {ssid}. The new system will join it too. Press Enter.")
                        } else {
                            format!("Joined {ssid}, but the internet isn't reachable yet. Press r to check again.")
                        })
                    }
                    Err(e) => Some(format!("{e:#}")),
                };
            }
            Action::Apps => {
                // The bindle wizard itself: same screens as on the desktop.
                ratatui::restore();
                let picked = crate::tui::run(w.catalog, w.facts, Some(w.selection()));
                *terminal = ratatui::init();
                match picked {
                    Ok(Some(sel)) => {
                        w.apps = Some(sel);
                        w.aur = Some(w.aur_packages());
                    }
                    Ok(None) => {}
                    Err(e) => w.message = Some(format!("{e:#}")),
                }
            }
            Action::Partition if !w.live => {
                w.message =
                    Some("Not on the live ISO (a dry run): archinstall's disk editor isn't opened here.".into());
            }
            Action::Partition => {
                // archinstall's own screen, for this one step.
                ratatui::restore();
                let edited = super::manual::edit();
                *terminal = ratatui::init();
                match edited {
                    Ok(Some(layout)) => match layout.check(&w.medium, w.uefi) {
                        Ok(()) => w.manual = Some(layout),
                        Err(why) => w.message = Some(format!("Layout not used: {why}.")),
                    },
                    Ok(None) => w.message = Some("No layout chosen in the editor; nothing changed.".into()),
                    Err(e) => w.message = Some(format!("{e:#}")),
                }
            }
            Action::Rescan => {
                let (disks, medium) = livesys::disks();
                w.set_disks(disks, medium);
                w.message = Some(format!("Found {} disk(s) to install to.", w.disks.len()));
            }
            Action::Keymap(_) if !w.live => {}
            Action::Keymap(layout) => {
                if let Err(e) = livesys::load_keys(&layout) {
                    w.message = Some(format!("{e:#}"));
                }
            }
            Action::Install => match w.answers() {
                Ok(answers) => {
                    let disk = w.manual.is_none().then(|| w.chosen_disk().cloned()).flatten();
                    return Ok(Some(Outcome { choice: w.choice(), answers, selection: w.selection(), disk }));
                }
                Err(e) => w.message = Some(e),
            },
        }
    }
}
