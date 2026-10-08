//! Terminal wizard: Install → Profile → Pick → Review. Returns the selection
//! when the user presses `a` to apply; the wizard itself never modifies the
//! system (applying happens in the plain terminal so sudo can prompt).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use bindle_core::catalog::Risk;
use bindle_core::{Catalog, Facts, Selection, plan};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::print;

pub mod iso;

#[derive(PartialEq)]
enum Screen {
    Install,
    Profile,
    Pick,
    Review,
}

struct App<'a> {
    catalog: &'a Catalog,
    facts: &'a Facts,
    screen: Screen,
    profile: ListState,
    category: ListState,
    module: ListState,
    focus_modules: bool,
    selected: BTreeSet<String>,
    provided: BTreeMap<String, String>,
    install: Vec<String>,
    review: Vec<String>,
    plan_ok: bool,
    seed: Option<Selection>,
    scroll: u16,
    quit: bool,
    apply: bool,
}

fn new_app<'a>(catalog: &'a Catalog, facts: &'a Facts, seed: Option<Selection>) -> App<'a> {
    let fallback = if facts.desktops.is_empty() { "minimal" } else { "desktop" };
    let suggested = seed.as_ref().and_then(|s| s.profile.as_deref()).unwrap_or(fallback);
    let suggested = catalog.profiles.iter().position(|p| p.id == suggested).unwrap_or(0);
    App {
        catalog,
        facts,
        screen: Screen::Install,
        profile: ListState::default().with_selected(Some(suggested)),
        category: ListState::default().with_selected(Some(0)),
        module: ListState::default().with_selected(Some(0)),
        focus_modules: false,
        selected: BTreeSet::new(),
        provided: facts.install.provided(),
        install: print::install_lines(facts),
        review: Vec::new(),
        plan_ok: false,
        seed,
        scroll: 0,
        quit: false,
        apply: false,
    }
}

/// `seed` pre-selects a profile and module tweaks (e.g. chosen before install).
pub fn run(catalog: &Catalog, facts: &Facts, seed: Option<Selection>) -> Result<Option<Selection>> {
    let mut app = new_app(catalog, facts, seed);
    let mut terminal = ratatui::init();
    let result = app.event_loop(&mut terminal);
    ratatui::restore();
    result?;
    if app.apply {
        return Ok(Some(app.selection()));
    }
    if app.screen == Screen::Review {
        println!("Equivalent command:\n  {}", app.command());
    }
    Ok(None)
}

impl<'a> App<'a> {
    fn event_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            terminal.draw(|f| self.draw(f))?;
            if let Event::Key(k) = event::read()?
                && k.kind == KeyEventKind::Press
            {
                self.on_key(k.code);
            }
        }
        Ok(())
    }

    fn profile_id(&self) -> String {
        self.catalog.profiles[self.profile.selected().unwrap_or(0)].id.clone()
    }

    fn base_selection(&self) -> BTreeSet<String> {
        let sel = Selection { profile: Some(self.profile_id()), ..Default::default() };
        sel.initial(self.catalog).unwrap_or_default()
    }

    fn selection(&self) -> Selection {
        let base = self.base_selection();
        Selection {
            profile: Some(self.profile_id()),
            include: self.selected.difference(&base).cloned().collect(),
            exclude: base.difference(&self.selected).cloned().collect(),
        }
    }

    fn command(&self) -> String {
        let s = self.selection();
        let mut cmd = format!("bindle plan --profile {}", self.profile_id());
        s.include.iter().for_each(|m| cmd.push_str(&format!(" --with {m}")));
        s.exclude.iter().for_each(|m| cmd.push_str(&format!(" --without {m}")));
        cmd
    }

    fn current_modules(&self) -> Vec<&'a bindle_core::catalog::Module> {
        let catalog: &'a Catalog = self.catalog;
        let cat = &catalog.categories[self.category.selected().unwrap_or(0)];
        catalog.modules_in(&cat.id).collect()
    }

    fn on_key(&mut self, key: KeyCode) {
        match (&self.screen, key) {
            (_, KeyCode::Char('q')) => self.quit = true,
            (Screen::Install, KeyCode::Enter) => self.screen = Screen::Profile,
            (Screen::Install, KeyCode::Up) => self.scroll = self.scroll.saturating_sub(1),
            (Screen::Install, KeyCode::Down) => self.scroll = self.scroll.saturating_add(1),
            (Screen::Profile, KeyCode::Esc) => self.screen = Screen::Install,
            (Screen::Profile, KeyCode::Up) => self.profile.select_previous(),
            (Screen::Profile, KeyCode::Down) => self.profile.select_next(),
            (Screen::Profile, KeyCode::Enter) => {
                self.selected = self.base_selection();
                if let Some(seed) = self.seed.as_ref().filter(|s| s.profile.as_deref() == Some(&self.profile_id())) {
                    self.selected.extend(seed.include.iter().cloned());
                    self.selected.retain(|m| !seed.exclude.contains(m));
                }
                self.screen = Screen::Pick;
            }
            (Screen::Pick, KeyCode::Esc) => self.screen = Screen::Profile,
            (Screen::Pick, KeyCode::Tab | KeyCode::Left | KeyCode::Right) => {
                self.focus_modules = !self.focus_modules;
            }
            (Screen::Pick, KeyCode::Up) if self.focus_modules => self.module.select_previous(),
            (Screen::Pick, KeyCode::Down) if self.focus_modules => {
                let max = self.current_modules().len().saturating_sub(1);
                self.module.select(Some((self.module.selected().unwrap_or(0) + 1).min(max)));
            }
            (Screen::Pick, KeyCode::Up) => {
                self.category.select_previous();
                self.module.select(Some(0));
            }
            (Screen::Pick, KeyCode::Down) => {
                let max = self.catalog.categories.len() - 1;
                self.category.select(Some((self.category.selected().unwrap_or(0) + 1).min(max)));
                self.module.select(Some(0));
            }
            (Screen::Pick, KeyCode::Char(' ')) if self.focus_modules => {
                if let Some(m) = self.current_modules().get(self.module.selected().unwrap_or(0)) {
                    let id = m.id.clone();
                    if self.provided.contains_key(&id) {
                        // Set up by the installer; nothing to toggle.
                    } else if !self.selected.remove(&id) {
                        self.selected.insert(id);
                    }
                }
            }
            (Screen::Pick, KeyCode::Enter) => {
                self.review = match plan::resolve(self.catalog, self.facts, &self.selection(), &|p| {
                    std::fs::read_to_string(p).ok()
                }) {
                    Ok(p) => {
                        self.plan_ok = !p.is_empty();
                        print::plan_lines(&p)
                    }
                    Err(e) => {
                        self.plan_ok = false;
                        vec![format!("Cannot build a plan: {e}"), String::new(), "Esc to go back.".into()]
                    }
                };
                self.scroll = 0;
                self.screen = Screen::Review;
            }
            (Screen::Review, KeyCode::Esc) => self.screen = Screen::Pick,
            (Screen::Review, KeyCode::Char('a')) if self.plan_ok => {
                self.apply = true;
                self.quit = true;
            }
            (Screen::Review, KeyCode::Up) => self.scroll = self.scroll.saturating_sub(1),
            (Screen::Review, KeyCode::Down) => self.scroll = self.scroll.saturating_add(1),
            (Screen::Review, KeyCode::PageDown) => self.scroll = self.scroll.saturating_add(15),
            (Screen::Review, KeyCode::PageUp) => self.scroll = self.scroll.saturating_sub(15),
            _ => {}
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let [header, body, footer] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
        let title = format!(
            " bindle · {} · {} · {:.0} GiB RAM · {} ",
            self.facts.pretty_name,
            self.facts.cpu_vendor,
            self.facts.ram_mib as f64 / 1024.0,
            if self.facts.gpus.is_empty() { "no gpu".into() } else { self.facts.gpus.join("+") }
        );
        f.render_widget(Line::from(title).bold().fg(Color::Yellow), header);
        let hint = match self.screen {
            Screen::Install => "Enter continue · ↑↓ scroll · q quit",
            Screen::Profile => "↑↓ choose · Enter continue · q quit",
            Screen::Pick => "Tab switch pane · ↑↓ move · Space toggle · Enter review · Esc back · q quit",
            Screen::Review => "a apply · ↑↓/PgUp/PgDn scroll · Esc back · q quit (prints the command)",
        };
        f.render_widget(Line::from(hint).dim(), footer);
        match self.screen {
            Screen::Install => {
                let text: Vec<Line> = self
                    .install
                    .iter()
                    .map(|l| match l.trim_start().chars().next() {
                        Some('✓') => Line::from(l.as_str()).fg(Color::Green),
                        Some('•') => Line::from(l.as_str()).fg(Color::Yellow),
                        _ => Line::from(l.as_str()),
                    })
                    .collect();
                let p = Paragraph::new(text)
                    .block(Block::bordered().title(" Your install "))
                    .wrap(Wrap { trim: false })
                    .scroll((self.scroll, 0));
                f.render_widget(p, body);
            }
            Screen::Profile => self.draw_profiles(f, body),
            Screen::Pick => self.draw_pick(f, body),
            Screen::Review => {
                let text: Vec<Line> = self.review.iter().map(|l| Line::from(l.as_str())).collect();
                let p = Paragraph::new(text)
                    .block(Block::bordered().title(" Review — nothing changes until you press a "))
                    .wrap(Wrap { trim: false })
                    .scroll((self.scroll, 0));
                f.render_widget(p, body);
            }
        }
    }

    fn draw_profiles(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        let items: Vec<ListItem> = self
            .catalog
            .profiles
            .iter()
            .map(|p| ListItem::new(vec![Line::from(p.name.as_str()).bold(), Line::from(p.description.as_str()).dim()]))
            .collect();
        let list = List::new(items)
            .block(Block::bordered().title(" Choose a starting profile "))
            .highlight_style(Style::new().bg(Color::DarkGray))
            .highlight_symbol("▸ ");
        f.render_stateful_widget(list, area, &mut self.profile);
    }

    fn draw_pick(&mut self, f: &mut Frame, area: ratatui::layout::Rect) {
        // Narrow terminals (first login opens at 80×24): details go below the lists.
        let (left, right, info) = if area.width < 100 {
            let [lists, info] = Layout::vertical([Constraint::Min(8), Constraint::Length(7)]).areas(area);
            let [left, right] = Layout::horizontal([Constraint::Length(22), Constraint::Min(30)]).areas(lists);
            (left, right, info)
        } else {
            let [left, right, info] =
                Layout::horizontal([Constraint::Length(24), Constraint::Min(30), Constraint::Percentage(35)])
                    .areas(area);
            (left, right, info)
        };
        let focused = |on: bool| {
            if on { Style::new().fg(Color::Yellow) } else { Style::new() }
        };

        let cats: Vec<ListItem> = self
            .catalog
            .categories
            .iter()
            .map(|c| {
                let n = self.catalog.modules_in(&c.id).filter(|m| self.selected.contains(&m.id)).count();
                ListItem::new(format!("{} ({n})", c.name))
            })
            .collect();
        let cats = List::new(cats)
            .block(Block::bordered().title(" Categories ").border_style(focused(!self.focus_modules)))
            .highlight_style(Style::new().bg(Color::DarkGray));
        f.render_stateful_widget(cats, left, &mut self.category);

        let modules = self.current_modules();
        let items: Vec<ListItem> = modules
            .iter()
            .map(|m| {
                let done = self.provided.contains_key(&m.id);
                let mark = if done {
                    "[✓]"
                } else if self.selected.contains(&m.id) {
                    "[x]"
                } else {
                    "[ ]"
                };
                let mut spans = vec![Span::raw(format!("{mark} {}", m.name))];
                match m.risk {
                    Risk::Safe => {}
                    Risk::Moderate => spans.push(Span::raw(" ●").fg(Color::Yellow)),
                    Risk::Advanced => spans.push(Span::raw(" ●").fg(Color::Red)),
                }
                let mut line = Line::from(spans);
                if done {
                    line = line.fg(Color::Green);
                } else if self.facts.matches(&m.when).is_err() {
                    line = line.add_modifier(Modifier::DIM);
                }
                ListItem::new(line)
            })
            .collect();
        let detail = modules.get(self.module.selected().unwrap_or(0)).map(|m| {
            let mut lines =
                vec![Line::from(m.name.as_str()).bold(), Line::from(""), Line::from(m.description.as_str())];
            if let Some(why) = self.provided.get(&m.id) {
                lines.push(Line::from(format!("Already set up: {why}")).fg(Color::Green));
            } else if let Err(r) = self.facts.matches(&m.when) {
                lines.push(Line::from(format!("Not applicable here: {r}")).fg(Color::Red));
            }
            if !m.requires.is_empty() {
                lines.push(Line::from(format!("Requires: {}", m.requires.join(", "))).dim());
            }
            if !m.conflicts.is_empty() {
                lines.push(Line::from(format!("Conflicts: {}", m.conflicts.join(", "))).dim());
            }
            lines.extend(m.notes.iter().map(|n| Line::from(format!("• {n}")).dim()));
            lines
        });
        let list = List::new(items)
            .block(Block::bordered().title(" Modules ").border_style(focused(self.focus_modules)))
            .highlight_style(if self.focus_modules { Style::new().bg(Color::DarkGray) } else { Style::new() });
        f.render_stateful_widget(list, right, &mut self.module);
        f.render_widget(
            Paragraph::new(detail.unwrap_or_default()).wrap(Wrap { trim: true }).block(Block::bordered()),
            info,
        );
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn screen_text(app: &mut App, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        term.backend().buffer().content().iter().map(|c| c.symbol()).collect::<String>()
    }

    /// First login opens in GNOME Console at 80×24; every screen must render there.
    #[test]
    fn every_screen_renders_at_80x24() {
        let catalog = Catalog::arch().unwrap();
        let facts = Facts { distro_id: "arch".into(), desktops: vec!["gnome".into()], ..Default::default() };
        let mut app = new_app(&catalog, &facts, None);
        assert!(screen_text(&mut app, 80, 24).contains("Your install"));
        app.on_key(KeyCode::Enter);
        assert!(screen_text(&mut app, 80, 24).contains("Desktop"));
        app.on_key(KeyCode::Enter);
        let pick = screen_text(&mut app, 80, 24);
        assert!(pick.contains("Categories") && pick.contains("Modules"));
        app.on_key(KeyCode::Tab);
        assert!(screen_text(&mut app, 80, 24).contains("[x]"));
        assert!(screen_text(&mut app, 160, 50).contains("Categories"));
        app.on_key(KeyCode::Enter);
        assert!(screen_text(&mut app, 80, 24).contains("Review"));
    }
}
