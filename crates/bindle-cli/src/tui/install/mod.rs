//! `bindle install`: the whole install on the live ISO in one wizard, in the
//! same look and keys as the `bindle` wizard. Network → Desktop → Hardware →
//! Disk → You → Apps → Review. Nothing is written until Install is confirmed on Review.

mod draw;
mod flow;
mod manual;
mod pages;
pub mod progress;

use anyhow::Result;
use bindle_core::catalog::Profile;
use bindle_core::desktops::Desktop;
use bindle_core::install::hardware::Hardware;
use bindle_core::install::preset::{self, IsoChoice};
use bindle_core::install::target::{self, Answers, Filesystem, MIN_DISK_BYTES};
use bindle_core::{Catalog, Facts, Selection};
use ratatui::crossterm::event::KeyCode;
use ratatui::widgets::ListState;

use crate::livesys::{self, Disk, WifiNetwork};
use manual::ManualDisk;

/// What the wizard hands back when the user confirms Install.
pub struct Outcome {
    pub choice: IsoChoice,
    pub answers: Answers,
    pub selection: Selection,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Step {
    Network,
    Desktop,
    Hardware,
    Disk,
    You,
    Apps,
    Review,
}

pub(crate) const STEPS: [(Step, &str); 7] = [
    (Step::Network, "Network"),
    (Step::Desktop, "Desktop"),
    (Step::Hardware, "Hardware"),
    (Step::Disk, "Disk"),
    (Step::You, "You"),
    (Step::Apps, "Apps"),
    (Step::Review, "Review"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Key {
    Hostname,
    Username,
    Password,
    Confirm,
    DiskPassphrase,
    Timezone,
    Keyboard,
    Language,
}

pub(crate) struct Field {
    pub key: Key,
    pub label: &'static str,
    pub value: String,
    pub secret: bool,
}

/// Something the event loop does outside the wizard's state.
#[derive(Debug, PartialEq)]
enum Action {
    None,
    Quit,
    Scan,
    Connect(String, Option<String>),
    Apps,
    /// Open archinstall's disk editor (manual partitioning).
    Partition,
    Install,
}

pub(crate) struct Wizard<'a> {
    pub facts: &'a Facts,
    pub catalog: &'a Catalog,
    pub desktops: &'a [Desktop],
    pub step: Step,
    pub online: bool,
    pub wifi: Option<String>,
    pub networks: Vec<WifiNetwork>,
    pub net: ListState,
    /// Typing the passphrase for the highlighted network.
    pub passphrase: Option<String>,
    pub desktop: ListState,
    pub profile: ListState,
    pub focus_profile: bool,
    /// Hardware rows the user changed; None: the recommendation.
    pub hw: Option<Hardware>,
    pub hw_row: ListState,
    pub app: ListState,
    pub disks: Vec<Disk>,
    pub disk: ListState,
    pub filesystem: Filesystem,
    pub encrypt: bool,
    /// A layout from archinstall's disk editor, instead of the whole disk.
    pub manual: Option<ManualDisk>,
    pub fields: Vec<Field>,
    pub field: usize,
    /// Apps unticked on the Apps page or chosen in the bindle wizard (`m`
    /// there), else the profile's.
    pub apps: Option<Selection>,
    pub confirm: bool,
    pub message: Option<String>,
    pub uefi: bool,
}

fn field(key: Key, label: &'static str, value: &str, secret: bool) -> Field {
    Field { key, label, value: value.into(), secret }
}

impl<'a> Wizard<'a> {
    pub fn new(facts: &'a Facts, catalog: &'a Catalog, desktops: &'a [Desktop]) -> Self {
        let profiles = &catalog.profiles;
        Wizard {
            facts,
            catalog,
            desktops,
            step: Step::Network,
            online: false,
            wifi: None,
            networks: Vec::new(),
            net: ListState::default().with_selected(Some(0)),
            passphrase: None,
            desktop: ListState::default().with_selected(Some(super::iso::desktop_row(desktops, None))),
            profile: ListState::default().with_selected(Some(super::iso::profile_index(profiles, None))),
            focus_profile: false,
            hw: None,
            hw_row: ListState::default().with_selected(Some(0)),
            app: ListState::default().with_selected(Some(0)),
            disks: Vec::new(),
            disk: ListState::default().with_selected(Some(0)),
            filesystem: Filesystem::Btrfs,
            encrypt: false,
            manual: None,
            fields: vec![
                field(Key::Hostname, "Computer name", "archlinux", false),
                field(Key::Username, "Your user name", "", false),
                field(Key::Password, "Password", "", true),
                field(Key::Confirm, "Password again", "", true),
                field(Key::DiskPassphrase, "Disk passphrase", "", true),
                field(Key::Timezone, "Timezone", "UTC", false),
                field(Key::Keyboard, "Keyboard layout", "us", false),
                field(Key::Language, "Language", "en_US", false),
            ],
            field: 0,
            apps: None,
            confirm: false,
            message: None,
            uefi: true,
        }
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.catalog.profiles
    }

    pub fn choice(&self) -> IsoChoice {
        let profiles = self.profiles();
        IsoChoice {
            desktop: self.desktop.selected().filter(|&i| i > 0).map(|i| self.desktops[i - 1].id.clone()),
            profile: profiles[self.profile.selected().unwrap_or(0).min(profiles.len() - 1)].id.clone(),
        }
    }

    pub fn selection(&self) -> Selection {
        self.apps.clone().unwrap_or_else(|| preset::first_login_selection(self.desktops, &self.choice()))
    }

    /// Bindle's own layout with encryption on (a manual layout brings its own).
    pub fn encrypting(&self) -> bool {
        self.encrypt && self.manual.is_none()
    }

    /// The fields shown: the disk passphrase only with encryption on.
    pub fn visible(&self) -> Vec<usize> {
        (0..self.fields.len()).filter(|&i| self.encrypting() || self.fields[i].key != Key::DiskPassphrase).collect()
    }

    fn value(&self, key: Key) -> &str {
        self.fields.iter().find(|f| f.key == key).map_or("", |f| f.value.as_str())
    }

    /// Why the You page isn't complete, or the answers.
    pub fn answers(&self) -> Result<Answers, String> {
        let disk = match (&self.manual, self.disks.get(self.disk.selected().unwrap_or(0))) {
            (Some(m), _) => Disk { path: m.devices(), bytes: 0, model: String::new(), transport: String::new() },
            (None, Some(d)) => d.clone(),
            (None, None) => return Err("choose a disk".into()),
        };
        let v = |k| self.value(k).trim().to_string();
        if !target::valid_hostname(&v(Key::Hostname)) {
            return Err("Computer name: lower-case letters, digits and -, up to 63".into());
        }
        if !target::valid_username(&v(Key::Username)) {
            return Err("User name: lower-case letters, digits, - and _, starting with a letter".into());
        }
        if self.value(Key::Password).is_empty() || self.value(Key::Password) != self.value(Key::Confirm) {
            return Err("The two passwords must match and can't be empty".into());
        }
        if self.encrypting() && self.value(Key::DiskPassphrase).len() < 8 {
            return Err("Disk passphrase: at least 8 characters".into());
        }
        if !livesys::valid_timezone(&v(Key::Timezone)) {
            return Err(format!("Timezone `{}` isn't known; use Area/City, e.g. Europe/London", v(Key::Timezone)));
        }
        Ok(Answers {
            disk: disk.path,
            disk_bytes: disk.bytes,
            filesystem: self.filesystem,
            encryption: match &self.manual {
                Some(m) => m.passphrase.clone(),
                None => self.encrypt.then(|| self.value(Key::DiskPassphrase).to_string()),
            },
            manual_disk: self.manual.as_ref().map(|m| m.config.clone()),
            uefi: self.uefi,
            hostname: v(Key::Hostname),
            username: v(Key::Username),
            password: self.value(Key::Password).to_string(),
            timezone: v(Key::Timezone),
            keyboard: v(Key::Keyboard),
            locale: v(Key::Language),
            hardware: self.hardware(),
        })
    }

    fn go(&mut self, step: Step) {
        self.step = step;
        self.confirm = false;
        self.message = None;
    }

    fn next(&mut self) {
        let next = match self.step {
            Step::Network if !self.online => {
                self.message = Some("Connect to the internet first: the install downloads everything.".into());
                return;
            }
            Step::Network => Step::Desktop,
            Step::Desktop => {
                if let Some(why) =
                    self.desktop.selected().filter(|&i| i > 0).and_then(|i| self.desktops[i - 1].broken.clone())
                {
                    self.message = Some(why);
                    return;
                }
                Step::Hardware
            }
            Step::Hardware => Step::Disk,
            Step::Disk => match self.disks.get(self.disk.selected().unwrap_or(0)) {
                _ if self.manual.is_some() => Step::You,
                None => {
                    self.message = Some("No disk to install to.".into());
                    return;
                }
                Some(d) if d.bytes < MIN_DISK_BYTES => {
                    self.message = Some("That disk is too small (20 GiB at least).".into());
                    return;
                }
                Some(_) => Step::You,
            },
            Step::You => match self.answers() {
                Ok(_) => Step::Apps,
                Err(e) => {
                    self.message = Some(e);
                    return;
                }
            },
            Step::Apps => Step::Review,
            Step::Review => return,
        };
        self.go(next);
    }

    fn back(&mut self) {
        let i = STEPS.iter().position(|(s, _)| *s == self.step).unwrap_or(0);
        if i > 0 {
            self.go(STEPS[i - 1].0);
        }
    }

    fn on_key(&mut self, key: KeyCode) -> Action {
        // Typing: the passphrase box and the You page take characters.
        if let Some(p) = self.passphrase.as_mut() {
            match key {
                KeyCode::Char(c) => p.push(c),
                KeyCode::Backspace => drop(p.pop()),
                KeyCode::Esc => self.passphrase = None,
                KeyCode::Enter => {
                    let ssid = self.networks.get(self.net.selected().unwrap_or(0)).map(|n| n.ssid.clone());
                    let pass = self.passphrase.take();
                    return ssid.map_or(Action::None, |s| Action::Connect(s, pass));
                }
                _ => {}
            }
            return Action::None;
        }
        if self.step == Step::You {
            let vis = self.visible();
            let pos = vis.iter().position(|&i| i == self.field).unwrap_or(0);
            match key {
                KeyCode::Up => self.field = vis[pos.saturating_sub(1)],
                KeyCode::Down | KeyCode::Tab => self.field = vis[(pos + 1).min(vis.len() - 1)],
                KeyCode::Enter if pos + 1 < vis.len() => self.field = vis[pos + 1],
                KeyCode::Enter => self.next(),
                KeyCode::Esc => self.back(),
                KeyCode::Backspace => drop(self.fields[self.field].value.pop()),
                KeyCode::Char(c) => self.fields[self.field].value.push(c),
                _ => {}
            }
            return Action::None;
        }
        if self.confirm {
            self.confirm = false;
            return if matches!(key, KeyCode::Char('y' | 'Y')) { Action::Install } else { Action::None };
        }
        self.message = None;
        if let Some(action) = self.on_page_key(key) {
            return action;
        }
        match (self.step, key) {
            (_, KeyCode::Char('q')) => return Action::Quit,
            (_, KeyCode::Esc | KeyCode::Left) if self.step != Step::Desktop => self.back(),
            (Step::Desktop, KeyCode::Esc) => self.back(),
            (Step::Network, KeyCode::Char('r')) => return Action::Scan,
            (Step::Network, KeyCode::Up) => self.net.select_previous(),
            (Step::Network, KeyCode::Down) => {
                let max = self.networks.len().saturating_sub(1);
                self.net.select(Some((self.net.selected().unwrap_or(0) + 1).min(max)));
            }
            (Step::Network, KeyCode::Char('c')) => match self.networks.get(self.net.selected().unwrap_or(0)) {
                Some(n) if n.security == "open" => return Action::Connect(n.ssid.clone(), None),
                Some(n) if n.security == "psk" => self.passphrase = Some(String::new()),
                Some(_) => self.message = Some("Enterprise Wi-Fi isn't supported here; use iwctl.".into()),
                None => {}
            },
            (Step::Desktop, KeyCode::Tab | KeyCode::Left | KeyCode::Right) => self.focus_profile = !self.focus_profile,
            (Step::Desktop, KeyCode::Up) => {
                self.desk_list().select_previous();
                self.reset_choices();
            }
            (Step::Desktop, KeyCode::Down) => {
                let max = if self.focus_profile { self.profiles().len() - 1 } else { self.desktops.len() };
                let l = self.desk_list();
                l.select(Some((l.selected().unwrap_or(0) + 1).min(max)));
                self.reset_choices();
            }
            (Step::Review, KeyCode::Char('m')) => return Action::Apps,
            (Step::Review, KeyCode::Char('i') | KeyCode::Enter) => self.confirm = true,
            (_, KeyCode::Enter | KeyCode::Right) => self.next(),
            _ => {}
        }
        Action::None
    }

    /// Another desktop or profile: its own recommendations and apps.
    fn reset_choices(&mut self) {
        self.apps = None;
        self.hw = None;
        self.hw_row.select(Some(0));
        self.app.select(Some(0));
    }

    fn desk_list(&mut self) -> &mut ListState {
        if self.focus_profile { &mut self.profile } else { &mut self.desktop }
    }
}

pub use flow::run;

#[cfg(test)]
mod tests;
