//! Pre-install screen for `bindle iso`: pick a desktop and a Bindle profile,
//! see what archinstall will be pre-filled with.

use anyhow::Result;
use bindle_core::Facts;
use bindle_core::catalog::Profile;
use bindle_core::desktops::Desktop;
use bindle_core::install::preset::{self, IsoChoice};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, layout::Rect};

pub fn run(
    facts: &Facts,
    desktops: &[Desktop],
    profiles: &[Profile],
    desktop: Option<String>,
    profile: Option<String>,
    overlay: Option<&serde_json::Value>,
) -> Result<Option<IsoChoice>> {
    if profiles.is_empty() {
        anyhow::bail!("the catalog has no profiles");
    }
    let mut d_state = ListState::default().with_selected(Some(desktop_row(desktops, desktop.as_deref())));
    let mut p_state = ListState::default().with_selected(Some(profile_index(profiles, profile.as_deref())));
    let mut focus_profile = false;
    let mut terminal = ratatui::init();
    let result = loop {
        let choice = IsoChoice {
            desktop: d_state.selected().filter(|&i| i > 0).map(|i| desktops[i - 1].id.clone()),
            profile: profiles[p_state.selected().unwrap_or(0).min(profiles.len() - 1)].id.clone(),
        };
        let summary = preset::summary(&preset::build(facts, desktops, &choice, overlay));
        let view = View { facts, desktops, profiles, summary: &summary, focus_profile };
        let drawn = terminal.draw(|f| view.draw(f, &mut d_state, &mut p_state));
        if let Err(e) = drawn {
            break Err(e.into());
        }
        let key = match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => k.code,
            Ok(_) => continue,
            Err(e) => break Err(e.into()),
        };
        let state = if focus_profile { &mut p_state } else { &mut d_state };
        match key {
            KeyCode::Char('q') | KeyCode::Esc => break Ok(None),
            KeyCode::Tab | KeyCode::Left | KeyCode::Right => focus_profile = !focus_profile,
            KeyCode::Up => state.select_previous(),
            KeyCode::Down => {
                let max = if focus_profile { profiles.len() - 1 } else { desktops.len() };
                state.select(Some((state.selected().unwrap_or(0) + 1).min(max)));
            }
            KeyCode::Enter => {
                let broken = d_state.selected().filter(|&i| i > 0).and_then(|i| desktops[i - 1].broken.as_ref());
                if broken.is_none() {
                    break Ok(Some(choice));
                }
            }
            _ => {}
        }
    };
    ratatui::restore();
    result
}

/// Row 0 = "choose in archinstall"; rows 1.. = desktops. The requested
/// desktop, else Bindle's recommended one, else row 0.
fn desktop_row(desktops: &[Desktop], wanted: Option<&str>) -> usize {
    let recommended = || bindle_core::desktops::recommended(desktops).map(|d| d.id.as_str());
    wanted.or_else(recommended).and_then(|id| desktops.iter().position(|d| d.id == id)).map_or(0, |i| i + 1)
}

/// The requested profile, else `desktop` (the everyday default), else the first.
fn profile_index(profiles: &[Profile], wanted: Option<&str>) -> usize {
    let pos = |id: &str| profiles.iter().position(|p| p.id == id);
    wanted.and_then(pos).or_else(|| pos("desktop")).unwrap_or(0)
}

struct View<'a> {
    facts: &'a Facts,
    desktops: &'a [Desktop],
    profiles: &'a [Profile],
    summary: &'a [String],
    focus_profile: bool,
}

impl View<'_> {
    fn draw(&self, f: &mut Frame, d_state: &mut ListState, p_state: &mut ListState) {
        let facts = self.facts;
        let [head, body, foot] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
        // The live ISO console can be 80 columns: stack the summary under the lists there.
        let (left, mid, right) = if body.width >= 110 {
            let [l, m, r] =
                Layout::horizontal([Constraint::Length(30), Constraint::Length(40), Constraint::Min(30)]).areas(body);
            (l, m, r)
        } else {
            let lists_h = (body.height * 2 / 5).max(7);
            let [lists, r] = Layout::vertical([Constraint::Length(lists_h), Constraint::Min(0)]).areas(body);
            let [l, m] = Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(lists);
            (l, m, r)
        };
        let hw = format!(
            " bindle installer · {} · {} threads · {:.0} GiB · GPU {} · {}",
            facts.cpu_vendor,
            facts.cpu_cores,
            facts.ram_mib as f64 / 1024.0,
            if facts.gpus.is_empty() { "none".into() } else { facts.gpus.join("+") },
            if facts.laptop { "laptop" } else { "desktop PC" }
        );
        f.render_widget(Line::from(hw).bold().fg(Color::Yellow), head);
        f.render_widget(Line::from("↑↓ choose · Tab switch list · Enter start archinstall · q quit").dim(), foot);
        let focused = |on: bool| if on { Style::new().fg(Color::Yellow) } else { Style::new() };
        let mut items = vec![ListItem::new("Choose in archinstall")];
        items.extend(self.desktops.iter().map(|d| match &d.broken {
            Some(_) => ListItem::new(format!("{} (unavailable)", d.name)).style(Style::new().fg(Color::DarkGray)),
            None if d.status.is_some() => ListItem::new(d.label()),
            None => ListItem::new(format!("{} ({})", d.name, d.display)),
        }));
        let list = List::new(items)
            .block(Block::bordered().title(" Desktop ").border_style(focused(!self.focus_profile)))
            .highlight_style(Style::new().bg(Color::DarkGray))
            .highlight_symbol("▸ ");
        f.render_stateful_widget(list, left, d_state);
        let items: Vec<ListItem> = self.profiles.iter().map(|p| ListItem::new(p.name.as_str())).collect();
        let list = List::new(items)
            .block(
                Block::bordered().title(" Bindle profile for first login ").border_style(focused(self.focus_profile)),
            )
            .highlight_style(Style::new().bg(Color::DarkGray))
            .highlight_symbol("▸ ");
        f.render_stateful_widget(list, mid, p_state);
        self.draw_summary(f, right, d_state, p_state);
    }

    fn draw_summary(&self, f: &mut Frame, area: Rect, d_state: &ListState, p_state: &ListState) {
        let mut lines: Vec<Line> = Vec::new();
        let picked = d_state.selected().filter(|&i| i > 0).map(|i| &self.desktops[i - 1]);
        if let Some(why) = picked.and_then(|d| d.broken.as_ref()) {
            lines.push(Line::from(format!("Unavailable: {why}")).fg(Color::Red));
            lines.push(Line::from(""));
        }
        if let Some(d) = picked.filter(|d| d.base.is_some()) {
            let base = d.profile(self.desktops);
            lines.push(Line::from(format!(
                "{}: archinstall installs {}; Bindle adds {} on first login.",
                d.name,
                base.name,
                d.name
            )));
        }
        lines.extend(self.summary.iter().map(|l| Line::from(l.as_str())));
        lines.push(Line::from(""));
        lines.push(Line::from("You still choose disks, users, hostname and locale.").dim());
        if let Some(p) = self.profiles.get(p_state.selected().unwrap_or(0)) {
            lines.push(Line::from(format!("After reboot, Bindle opens with the {} profile.", p.name)).dim());
        }
        let block = Block::bordered().title(" archinstall will open pre-filled with ");
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), area);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    #[test]
    fn opens_on_the_requested_then_the_recommended_desktop() {
        let mut desktops = bindle_core::desktops::all().unwrap();
        let row = |d: &[Desktop], id: &str| d.iter().position(|x| x.id == id).unwrap() + 1;
        assert_eq!(desktop_row(&desktops, Some("gnome")), row(&desktops, "gnome"));
        // uQ is recommended, so it is pre-selected.
        assert_eq!(desktop_row(&desktops, None), row(&desktops, "uq"));
        // A preview desktop pre-selects nothing.
        let uq = desktops.iter_mut().find(|d| d.id == "uq").unwrap();
        uq.status = Some(bindle_core::desktops::Status::Preview);
        assert_eq!(desktop_row(&desktops, None), 0);
        assert_eq!(desktop_row(&desktops, Some("plasma")), row(&desktops, "plasma"));
    }

    #[test]
    fn renders_on_an_80_column_console() {
        let desktops = bindle_core::desktops::all().unwrap();
        let catalog = bindle_core::Catalog::arch().unwrap();
        let facts = Facts::default();
        let choice = IsoChoice { desktop: Some("plasma".into()), profile: "desktop".into() };
        let summary = preset::summary(&preset::build(&facts, &desktops, &choice, None));
        let view = View {
            facts: &facts,
            desktops: &desktops,
            profiles: &catalog.profiles,
            summary: &summary,
            focus_profile: false,
        };
        for (w, h) in [(80, 25), (128, 48)] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            let (mut d, mut p) =
                (ListState::default().with_selected(Some(1)), ListState::default().with_selected(Some(1)));
            term.draw(|f| view.draw(f, &mut d, &mut p)).unwrap();
            let text: String = term.backend().buffer().content().iter().map(|c| c.symbol()).collect();
            assert!(
                text.contains("Desktop") && text.contains("pre-filled") && text.contains("Desktop profile"),
                "{w}x{h}"
            );
        }
    }
}
