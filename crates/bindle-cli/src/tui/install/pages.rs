//! The Hardware and Apps pages of `bindle install`: what detection
//! recommends for this machine, each row changeable, and the apps the profile
//! installs, each one untickable. Also the Disk page's keys.

use bindle_core::Selection;
use bindle_core::catalog::Module;
use bindle_core::install::hardware::Hardware;
use bindle_core::install::target::Filesystem;
use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, ListItem, ListState, Paragraph, Wrap};

use super::draw::{list, panes};
use super::{Action, Step, Wizard};

fn step(state: &mut ListState, down: bool, len: usize) {
    let at = state.selected().unwrap_or(0);
    state.select(Some(if down { (at + 1).min(len.saturating_sub(1)) } else { at.saturating_sub(1) }));
}

impl<'a> Wizard<'a> {
    /// The Hardware page's rows: the user's changes, else the recommendation
    /// for the desktop and profile chosen so far.
    pub fn hardware(&self) -> Hardware {
        self.hw.clone().unwrap_or_else(|| {
            let choice = self.choice();
            let graphical = choice.desktop.is_some() || choice.profile != "minimal";
            Hardware::recommended(self.facts, self.uefi, graphical)
        })
    }

    /// Every app the profile and desktop bring, unticked ones included.
    pub fn app_modules(&self) -> Vec<&'a Module> {
        let catalog = self.catalog;
        let all = Selection { exclude: Vec::new(), ..self.selection() };
        let ids = all.initial(catalog).unwrap_or_default();
        ids.iter().filter_map(|id| catalog.module(id)).collect()
    }

    fn toggle_app(&mut self) {
        let Some(id) = self.app_modules().get(self.app.selected().unwrap_or(0)).map(|m| m.id.clone()) else { return };
        let mut sel = self.selection();
        match sel.exclude.iter().position(|x| *x == id) {
            Some(i) => drop(sel.exclude.remove(i)),
            None => sel.exclude.push(id),
        }
        self.apps = Some(sel);
    }

    /// Keys of the two pages. None: not theirs (Enter, Esc, q…).
    pub(super) fn on_page_key(&mut self, key: KeyCode) -> Option<Action> {
        match (self.step, key) {
            (Step::Hardware, KeyCode::Up | KeyCode::Down) => {
                let len = self.hardware().rows.len();
                step(&mut self.hw_row, key == KeyCode::Down, len);
            }
            (Step::Hardware, KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')) => {
                let mut hw = self.hardware();
                if let Some(r) = hw.rows.get_mut(self.hw_row.selected().unwrap_or(0)) {
                    r.cycle(key != KeyCode::Left);
                }
                self.hw = Some(hw);
            }
            (Step::Hardware, KeyCode::Char('r')) => self.hw = None,
            (Step::Apps, KeyCode::Up | KeyCode::Down) => {
                let len = self.app_modules().len();
                step(&mut self.app, key == KeyCode::Down, len);
            }
            (Step::Apps, KeyCode::Char(' ')) => self.toggle_app(),
            (Step::Apps, KeyCode::Char('m')) => return Some(Action::Apps),
            (Step::Disk, KeyCode::Char('p')) => return Some(Action::Partition),
            (Step::Disk, KeyCode::Char('x')) => self.manual = None,
            (Step::Disk, KeyCode::Up | KeyCode::Down | KeyCode::Char('f' | 'e')) if self.manual.is_some() => {
                self.message = Some("A manual layout is set: p edits it again, x goes back to Bindle's layout.".into());
            }
            (Step::Disk, KeyCode::Up | KeyCode::Down) => {
                let len = self.disks.len();
                step(&mut self.disk, key == KeyCode::Down, len);
            }
            (Step::Disk, KeyCode::Char('f')) => {
                self.filesystem =
                    if self.filesystem == Filesystem::Btrfs { Filesystem::Ext4 } else { Filesystem::Btrfs };
            }
            (Step::Disk, KeyCode::Char('e')) => self.encrypt = !self.encrypt,
            _ => return None,
        }
        Some(Action::None)
    }

    pub(super) fn draw_hardware(&mut self, f: &mut Frame, body: Rect) {
        let (left, right) = panes(body, 60);
        let hw = self.hardware();
        let items: Vec<ListItem> = hw
            .rows
            .iter()
            .map(|r| {
                let mark = if r.fixed() || r.selected == r.recommended { "" } else { "  (changed)" };
                ListItem::new(format!("{:<16} {}{mark}", r.label, r.choice().label))
            })
            .collect();
        f.render_stateful_widget(list(items, " Recommended for this hardware ", true), left, &mut self.hw_row);
        let mut lines: Vec<Line> = Vec::new();
        if let Some(r) = hw.rows.get(self.hw_row.selected().unwrap_or(0)) {
            lines.push(Line::from(r.label).bold());
            lines.push(Line::from(r.why.clone()));
            lines.push(Line::from(""));
            for (i, c) in r.choices.iter().enumerate() {
                let dot = if i == r.selected { "●" } else { "○" };
                let rec = if i == r.recommended && !r.fixed() { "  recommended" } else { "" };
                let line = Line::from(format!("{dot} {}{rec}", c.label));
                lines.push(if i == r.selected { line.fg(Color::Green) } else { line });
            }
            if r.fixed() {
                lines.push(Line::from(""));
                lines.push(Line::from("Set automatically; nothing to choose.").dim());
            }
        }
        let block = Block::bordered().title(" Details ");
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), right);
    }

    pub(super) fn draw_apps(&mut self, f: &mut Frame, body: Rect) {
        let (left, right) = panes(body, 44);
        let mods = self.app_modules();
        let off = self.selection().exclude;
        let items: Vec<ListItem> = mods
            .iter()
            .map(|m| ListItem::new(format!("[{}] {}", if off.contains(&m.id) { " " } else { "x" }, m.name)))
            .collect();
        f.render_stateful_widget(list(items, " Apps and setup ", true), left, &mut self.app);
        let mut lines: Vec<Line> = Vec::new();
        match mods.get(self.app.selected().unwrap_or(0)) {
            Some(m) => {
                lines.push(Line::from(m.name.clone()).bold());
                lines.push(Line::from(m.description.clone()));
            }
            None => lines.push(Line::from("This profile installs nothing extra.")),
        }
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "{} of {} will be installed.",
            mods.len() - off.len().min(mods.len()),
            mods.len()
        )));
        lines.push(Line::from("Anything that doesn't fit this hardware is skipped on its own.").dim());
        lines.push(Line::from("m opens the full Bindle wizard to add more; a there keeps your choice.").dim());
        let block = Block::bordered().title(" Details ");
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), right);
    }
}
