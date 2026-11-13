//! Terminal wizard: Install → Profile → Pick → Review. Returns the selection
//! when the user presses `a` to apply; the wizard itself never modifies the
//! system (applying happens in the plain terminal so sudo can prompt).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use bindle_core::{Catalog, Facts, Selection, plan};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::print;

mod pick;
mod wrap;

pub mod install;
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
    /// Scroll of the Pick screen's detail pane; reset when the module changes.
    detail_scroll: u16,
    /// One-line message on the Pick screen (e.g. a rival module was deselected).
    notice: Option<String>,
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
        detail_scroll: 0,
        notice: None,
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
            // The Review screen lists replacements before anything happens.
            replace_conflicts: true,
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
            (Screen::Pick, KeyCode::Up) if self.focus_modules => {
                self.module.select_previous();
                (self.detail_scroll, self.notice) = (0, None);
            }
            (Screen::Pick, KeyCode::Down) if self.focus_modules => {
                let max = self.current_modules().len().saturating_sub(1);
                self.module.select(Some((self.module.selected().unwrap_or(0) + 1).min(max)));
                (self.detail_scroll, self.notice) = (0, None);
            }
            (Screen::Pick, KeyCode::PageDown) => self.detail_scroll = self.detail_scroll.saturating_add(3),
            (Screen::Pick, KeyCode::PageUp) => self.detail_scroll = self.detail_scroll.saturating_sub(3),
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
                    let rivals = m.conflicts.clone();
                    if self.provided.contains_key(&id) || self.facts.matches(&m.when).is_err() {
                        // Set up by the installer, or skipped here; nothing to toggle.
                    } else if !self.selected.remove(&id) {
                        // Picking one of a conflicting pair drops the other.
                        let dropped: Vec<String> = rivals.into_iter().filter(|r| self.selected.remove(r)).collect();
                        if !dropped.is_empty() {
                            self.notice = Some(format!("{id} replaces {} (they conflict)", dropped.join(", ")));
                        }
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
                let width = usize::from(body.width.saturating_sub(2));
                let text: Vec<Line> = self
                    .review
                    .iter()
                    .flat_map(|l| wrap::hanging(l, width))
                    .map(|l| match l.trim_start().chars().next() {
                        Some('~' | '+') => Line::from(l).fg(Color::Yellow),
                        Some('-') => Line::from(l).dim(),
                        Some('✓') => Line::from(l).fg(Color::Green),
                        _ if l.starts_with("Steps") || l.starts_with("Notes") => Line::from(l).bold(),
                        _ => Line::from(l),
                    })
                    .collect();
                let p = Paragraph::new(text)
                    .block(Block::bordered().title(" Review: nothing changes until you press a "))
                    .scroll((self.scroll, 0));
                f.render_widget(p, body);
            }
        }
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

    #[test]
    fn a_detected_drive_can_be_ticked_and_the_review_shows_its_fstab_line() {
        use bindle_core::detect::Drive;
        let drive = |uuid: &str, fstype: &str, label: &str| Drive {
            device: "/dev/sdb1".into(),
            uuid: uuid.into(),
            fstype: fstype.into(),
            label: label.into(),
            size: 500 << 30,
            ..Default::default()
        };
        let locked = Drive { blocked: Some("encrypted with LUKS".into()), ..drive("cc", "crypto_LUKS", "") };
        let facts = Facts {
            distro_id: "arch".into(),
            drives: vec![drive("aa-1", "ext4", "Data"), locked],
            ..Default::default()
        };
        let catalog = Catalog::arch().unwrap().with_drives(&facts);
        let mut app = new_app(&catalog, &facts, None);
        app.screen = Screen::Pick;
        app.category.select(catalog.categories.iter().position(|c| c.id == "hardware"));
        let at = |id: &str| catalog.modules_in("hardware").position(|m| m.id == id);
        app.module.select(at("mount-aa-1"));
        app.focus_modules = true;
        app.on_key(KeyCode::Char(' '));
        assert!(app.selected.contains("mount-aa-1"));
        let text = screen_text(&mut app, 120, 60);
        assert!(text.contains("[x] Mount Data (500 GiB ext4)"), "{text}");
        assert!(text.contains("/mnt/Data every time the computer"), "{text}");
        assert!(text.contains("Adds to /etc/fstab: UUID=aa-1 /mnt/Data"), "{text}");
        // An encrypted volume is listed, says why, and can't be ticked.
        app.module.select(at("mount-cc"));
        app.on_key(KeyCode::Char(' '));
        assert!(!app.selected.contains("mount-cc"));
        assert!(screen_text(&mut app, 120, 60).contains("Not available here: encrypted with LUKS"));
        app.on_key(KeyCode::Enter);
        let review = app.review.join("\n");
        assert!(review.contains("add to /etc/fstab, to mount at startup: /mnt/Data"), "{review}");
        assert!(
            review.contains("│ UUID=aa-1  /mnt/Data  ext4  defaults,nofail,x-systemd.device-timeout=30s  0 2"),
            "{review}"
        );
    }

    #[test]
    fn picking_one_of_a_conflicting_pair_drops_the_other() {
        let catalog = Catalog::arch().unwrap();
        let facts = Facts { distro_id: "arch".into(), laptop: true, ..Default::default() };
        let mut app = new_app(&catalog, &facts, None);
        let cat = catalog.categories.iter().position(|c| c.id == "hardware").unwrap();
        let tlp = catalog.modules_in("hardware").position(|m| m.id == "tlp").unwrap();
        app.selected.insert("power-profiles".into());
        app.screen = Screen::Pick;
        app.category.select(Some(cat));
        app.module.select(Some(tlp));
        app.focus_modules = true;
        app.on_key(KeyCode::Char(' '));
        assert!(app.selected.contains("tlp") && !app.selected.contains("power-profiles"));
        let text = screen_text(&mut app, 80, 24);
        assert!(text.contains("tlp replaces power-profiles"), "{text}");
        // The detail pane scrolls to reach the module's notes.
        app.on_key(KeyCode::PageDown);
        assert_eq!(app.detail_scroll, 3);
        app.on_key(KeyCode::Up);
        assert_eq!((app.detail_scroll, app.notice.is_none()), (0, true));
    }
}
