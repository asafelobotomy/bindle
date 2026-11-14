//! The keys of `bindle install`'s wizard: what each page does with them,
//! typing into the You page and the Wi-Fi password box, Ctrl-C, and the
//! keyboard layout switching the console as soon as it is set.

use bindle_core::install::target;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, Key, Step, Wizard};

impl Wizard<'_> {
    /// A key as the terminal sent it. Ctrl-C quits (asking first once
    /// something was typed); any other key with Ctrl or Alt held does
    /// nothing, so it never types into a field or answers the wipe question.
    pub(super) fn on_event(&mut self, k: KeyEvent) -> Action {
        let held = k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let ctrl_c = k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('c' | 'C'));
        if self.quit_ask {
            self.quit_ask = false;
            self.message = None;
            let yes = ctrl_c || (!held && matches!(k.code, KeyCode::Char('y' | 'Y')));
            return if yes { Action::Quit } else { Action::None };
        }
        if ctrl_c {
            if !self.typed && self.manual.is_none() {
                return Action::Quit;
            }
            self.confirm = false;
            self.quit_ask = true;
            self.message = Some("Quit the installer? Nothing has been changed. y quit · any other key stays".into());
            return Action::None;
        }
        if held {
            return Action::None;
        }
        self.on_key(k.code)
    }

    /// Leaving the keyboard field: switch the console to the new layout, so
    /// what follows (the passwords) is typed as it will be at boot.
    fn keyboard_set(&mut self) -> Action {
        let layout = self.value(Key::Keyboard).trim().to_string();
        if layout.is_empty() || layout == self.keymap {
            return Action::None;
        }
        if self.keymaps.as_ref().is_some_and(|m| !target::valid_keymap(&layout, m)) {
            self.message = Some(format!("Keyboard layout `{layout}` isn't known; e.g. us, uk, de-latin1, fr"));
            return Action::None;
        }
        if self.fields.iter().any(|f| f.secret && !f.value.is_empty()) {
            self.message =
                Some("The keyboard changed after passwords were typed: type them again, so they match at boot.".into());
        }
        self.keymap = layout.clone();
        Action::Keymap(layout)
    }

    pub(super) fn on_key(&mut self, key: KeyCode) -> Action {
        // Typing: the passphrase box and the You page take characters.
        if let Some(p) = self.passphrase.as_mut() {
            match key {
                KeyCode::Char(c) => {
                    p.push(c);
                    self.typed = true;
                }
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
            let was = self.field;
            match key {
                KeyCode::Up => self.field = vis[pos.saturating_sub(1)],
                KeyCode::Down | KeyCode::Tab => self.field = vis[(pos + 1).min(vis.len() - 1)],
                KeyCode::Enter if pos + 1 < vis.len() => self.field = vis[pos + 1],
                KeyCode::Enter => self.next(),
                KeyCode::Esc => self.back(),
                KeyCode::Backspace => drop(self.fields[self.field].value.pop()),
                KeyCode::Char(c) => {
                    self.fields[self.field].value.push(c);
                    self.typed = true;
                }
                _ => {}
            }
            if self.fields[was].key == Key::Keyboard && (self.field != was || self.step != Step::You) {
                return self.keyboard_set();
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
}
