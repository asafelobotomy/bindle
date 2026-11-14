//! `bindle install`: the whole install on the live ISO in one wizard, in the
//! same look and keys as the `bindle` wizard. Network → Desktop → Hardware →
//! Disk → You → Apps → Review. Nothing is written until Install is confirmed on Review.

mod answers;
mod draw;
mod flow;
pub mod guard;
mod keys;
mod manual;
mod pages;
pub mod progress;

use anyhow::Result;
use bindle_core::catalog::Profile;
use bindle_core::desktops::Desktop;
use bindle_core::install::hardware::Hardware;
use bindle_core::install::preset::{self, IsoChoice};
use bindle_core::install::target::{Answers, Filesystem, MIN_DISK_BYTES};
use bindle_core::{Catalog, Facts, Selection};
use ratatui::widgets::ListState;

use crate::livesys::{Disk, Medium, WifiNetwork};
use manual::ManualDisk;

/// What the wizard hands back when the user confirms Install.
pub struct Outcome {
    pub choice: IsoChoice,
    pub answers: Answers,
    pub selection: Selection,
    /// The whole disk to erase, as it was shown (None: a manual layout).
    pub disk: Option<Disk>,
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
    Keyboard,
    Hostname,
    Username,
    Password,
    Confirm,
    DiskPassphrase,
    DiskConfirm,
    Timezone,
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
    /// Look for disks again.
    Rescan,
    /// Switch the console to this keyboard layout (`loadkeys`).
    Keymap(String),
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
    /// Ctrl-C after typing something: asking before quitting.
    pub quit_ask: bool,
    /// Something was typed into a field (Ctrl-C asks before throwing it away).
    pub typed: bool,
    pub message: Option<String>,
    pub uefi: bool,
    /// On the live ISO: Wi-Fi, the disk editor and loadkeys are only used there.
    pub live: bool,
    /// The disk(s) the ISO booted from, left out of `disks`.
    pub medium: Medium,
    /// The ISO's keymaps and glibc's SUPPORTED list; None: not checked.
    pub keymaps: Option<Vec<String>>,
    pub locales: Option<String>,
    /// The ISO's /etc/passwd: system accounts a user name can't take.
    pub passwd: String,
    /// The keyboard layout the console was last switched to.
    pub keymap: String,
    /// AUR packages the plan builds, for Review (Err: why it couldn't tell).
    pub aur: Option<Result<Vec<String>, String>>,
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
            // The keyboard first: everything after it, passwords included,
            // is typed with the layout the new system will have.
            fields: vec![
                field(Key::Keyboard, "Keyboard layout", "us", false),
                field(Key::Hostname, "Computer name", "archlinux", false),
                field(Key::Username, "Your user name", "", false),
                field(Key::Password, "Password", "", true),
                field(Key::Confirm, "Password again", "", true),
                field(Key::DiskPassphrase, "Disk passphrase", "", true),
                field(Key::DiskConfirm, "Disk passphrase again", "", true),
                field(Key::Timezone, "Timezone", "UTC", false),
                field(Key::Language, "Language", "en_US", false),
            ],
            field: 0,
            apps: None,
            confirm: false,
            quit_ask: false,
            typed: false,
            message: None,
            uefi: true,
            live: false,
            medium: Medium::NotLive,
            keymaps: None,
            locales: None,
            passwd: String::new(),
            keymap: "us".into(),
            aur: None,
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
        let mut sel = self.apps.clone().unwrap_or_else(|| preset::first_login_selection(self.desktops, &self.choice()));
        // A driver the Hardware page was told not to use (open-source chosen).
        for m in self.hardware().excluded_modules() {
            if !sel.exclude.iter().any(|x| x == m) {
                sel.exclude.push(m.into());
            }
        }
        sel
    }

    /// Bindle's own layout with encryption on (a manual layout brings its own).
    pub fn encrypting(&self) -> bool {
        self.encrypt && self.manual.is_none()
    }

    /// The fields shown: the disk passphrase only with encryption on.
    pub fn visible(&self) -> Vec<usize> {
        let disk = |k| matches!(k, Key::DiskPassphrase | Key::DiskConfirm);
        (0..self.fields.len()).filter(|&i| self.encrypting() || !disk(self.fields[i].key)).collect()
    }

    /// The highlighted disk on the Disk page.
    pub fn chosen_disk(&self) -> Option<&Disk> {
        self.disks.get(self.disk.selected().unwrap_or(0))
    }

    /// New disks (after a rescan or at start), starting on the preferred one.
    pub fn set_disks(&mut self, disks: Vec<Disk>, medium: Medium) {
        self.disk.select(Some(crate::livesys::preferred(&disks)));
        self.disks = disks;
        self.medium = medium;
    }

    fn value(&self, key: Key) -> &str {
        self.fields.iter().find(|f| f.key == key).map_or("", |f| f.value.as_str())
    }

    fn go(&mut self, step: Step) {
        self.step = step;
        self.confirm = false;
        self.message = None;
        if step == Step::Review {
            self.aur = Some(self.aur_packages());
        }
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
            Step::Disk => match self.chosen_disk() {
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
