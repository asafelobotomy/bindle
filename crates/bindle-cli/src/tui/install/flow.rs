//! The event loop of `bindle install`: keys go to the wizard; scanning,
//! joining Wi-Fi and the apps screen happen here.

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

/// Run the wizard. None: the user quit.
pub fn run(facts: &Facts, catalog: &Catalog, desktops: &[Desktop]) -> Result<Option<Outcome>> {
    if catalog.profiles.is_empty() {
        anyhow::bail!("the catalog has no profiles");
    }
    let mut w = Wizard::new(facts, catalog, desktops);
    w.disks = livesys::disks();
    w.uefi = livesys::uefi();
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
        match w.on_key(k.code) {
            Action::None => {}
            Action::Quit => return Ok(None),
            Action::Scan => {
                w.message = Some("Looking for networks…".into());
                terminal.draw(|f| w.draw(f))?;
                refresh_network(w);
                w.message = None;
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
                    Ok(Some(sel)) => w.apps = Some(sel),
                    Ok(None) => {}
                    Err(e) => w.message = Some(format!("{e:#}")),
                }
            }
            Action::Partition => {
                // archinstall's own screen, for this one step.
                ratatui::restore();
                let edited = super::manual::edit();
                *terminal = ratatui::init();
                match edited {
                    Ok(Some(layout)) => w.manual = Some(layout),
                    Ok(None) => w.message = Some("No layout chosen in the editor; nothing changed.".into()),
                    Err(e) => w.message = Some(format!("{e:#}")),
                }
            }
            Action::Install => match w.answers() {
                Ok(answers) => return Ok(Some(Outcome { choice: w.choice(), answers, selection: w.selection() })),
                Err(e) => w.message = Some(e),
            },
        }
    }
}
