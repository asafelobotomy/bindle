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

pub fn run(
    facts: &Facts,
    desktops: &[Desktop],
    profiles: &[Profile],
    desktop: Option<String>,
    profile: Option<String>,
) -> Result<Option<IsoChoice>> {
    if profiles.is_empty() {
        anyhow::bail!("the catalog has no profiles");
    }
    // Row 0 = "choose in archinstall"; rows 1.. = desktops.
    let mut d_state = ListState::default()
        .with_selected(Some(desktop.and_then(|id| desktops.iter().position(|d| d.id == id)).map_or(0, |i| i + 1)));
    let mut p_state = ListState::default().with_selected(Some(profile_index(profiles, profile.as_deref())));
    let mut focus_profile = false;
    let mut terminal = ratatui::init();
    let result = loop {
        let choice = IsoChoice {
            desktop: d_state.selected().filter(|&i| i > 0).map(|i| desktops[i - 1].id.clone()),
            profile: profiles[p_state.selected().unwrap_or(0).min(profiles.len() - 1)].id.clone(),
        };
        let summary = preset::summary(&preset::archinstall_preset(facts, desktops, &choice));
        let drawn = terminal.draw(|f| {
            let [head, body, foot] =
                Layout::vertical([Constraint::Length(2), Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
            let [left, mid, right] =
                Layout::horizontal([Constraint::Length(30), Constraint::Length(40), Constraint::Min(30)]).areas(body);
            let hw = format!(
                " bindle installer · {} · {} threads · {:.0} GiB · GPU {} · {}",
                facts.cpu_vendor,
                facts.cpu_cores,
                facts.ram_mib as f64 / 1024.0,
                if facts.gpus.is_empty() { "none".into() } else { facts.gpus.join("+") },
                if facts.laptop { "laptop" } else { "desktop PC" }
            );
            f.render_widget(Paragraph::new(vec![Line::from(hw).bold().fg(Color::Yellow), Line::from("")]), head);
            f.render_widget(Line::from("↑↓ choose · Tab switch list · Enter start archinstall · q quit").dim(), foot);
            let focused = |on: bool| if on { Style::new().fg(Color::Yellow) } else { Style::new() };
            let mut items = vec![ListItem::new("Choose in archinstall")];
            items.extend(desktops.iter().map(|d| match &d.broken {
                Some(_) => ListItem::new(format!("{} (unavailable)", d.name)).style(Style::new().fg(Color::DarkGray)),
                None => ListItem::new(format!("{} ({})", d.name, d.display)),
            }));
            let list = List::new(items)
                .block(Block::bordered().title(" Desktop ").border_style(focused(!focus_profile)))
                .highlight_style(Style::new().bg(Color::DarkGray))
                .highlight_symbol("▸ ");
            f.render_stateful_widget(list, left, &mut d_state);
            let items: Vec<ListItem> = profiles
                .iter()
                .map(|p| {
                    ListItem::new(vec![Line::from(p.id.as_str()).bold(), Line::from(p.description.as_str()).dim()])
                })
                .collect();
            let list = List::new(items)
                .block(Block::bordered().title(" Bindle profile (first login) ").border_style(focused(focus_profile)))
                .highlight_style(Style::new().bg(Color::DarkGray))
                .highlight_symbol("▸ ");
            f.render_stateful_widget(list, mid, &mut p_state);
            let mut lines: Vec<Line> =
                vec![Line::from("archinstall will open pre-filled with:").bold(), Line::from("")];
            if let Some(why) = d_state.selected().filter(|&i| i > 0).and_then(|i| desktops[i - 1].broken.as_ref()) {
                lines.push(Line::from(format!("Unavailable: {why}")).fg(Color::Red));
                lines.push(Line::from(""));
            }
            lines.extend(summary.iter().map(|l| Line::from(l.as_str())));
            lines.push(Line::from(""));
            lines.push(Line::from("You still choose disks, users, hostname and locale.").dim());
            lines.push(Line::from("After reboot, Bindle opens with this profile.").dim());
            f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(Block::bordered()), right);
        });
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

/// The requested profile, else `desktop` (the everyday default), else the first.
fn profile_index(profiles: &[Profile], wanted: Option<&str>) -> usize {
    let pos = |id: &str| profiles.iter().position(|p| p.id == id);
    wanted.and_then(pos).or_else(|| pos("desktop")).unwrap_or(0)
}
