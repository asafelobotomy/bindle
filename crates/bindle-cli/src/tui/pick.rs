//! Pick screen (and the profile list): categories, modules, details.

use bindle_core::catalog::Risk;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph, Wrap};

use super::App;
use crate::print;

impl App<'_> {
    pub(super) fn draw_profiles(&mut self, f: &mut Frame, area: Rect) {
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

    pub(super) fn draw_pick(&mut self, f: &mut Frame, area: Rect) {
        // Narrow terminals (first login opens at 80×24): details go below the lists.
        let (left, right, info) = if area.width < 100 {
            let [lists, info] = Layout::vertical([Constraint::Min(7), Constraint::Length(10)]).areas(area);
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
        let detail = if self.focus_modules {
            modules.get(self.module.selected().unwrap_or(0)).map(|m| {
                let mut lines = Vec::new();
                if let Some(n) = &self.notice {
                    lines.push(Line::from(n.as_str()).fg(Color::Yellow));
                }
                lines.extend([Line::from(m.name.as_str()).bold(), Line::from(m.description.as_str())]);
                if let Some(why) = self.provided.get(&m.id) {
                    lines.push(Line::from(format!("Already set up: {why}")).fg(Color::Green));
                } else if let Err(r) = self.facts.matches(&m.when) {
                    lines.push(Line::from(format!("Not available here: {r}")).fg(Color::Red));
                }
                lines.extend(m.notes.iter().map(|n| Line::from(format!("• {n}")).fg(Color::Cyan)));
                lines.push(Line::from(""));
                lines.extend(print::module_details(m, self.facts).into_iter().map(|l| Line::from(l).dim()));
                if !m.requires.is_empty() {
                    lines.push(Line::from(format!("Requires: {}", m.requires.join(", "))).dim());
                }
                if !m.conflicts.is_empty() {
                    lines.push(Line::from(format!("Can't be combined with: {}", m.conflicts.join(", "))).dim());
                }
                lines
            })
        } else {
            self.catalog.categories.get(self.category.selected().unwrap_or(0)).map(|c| {
                let total = modules.len();
                let picked = modules.iter().filter(|m| self.selected.contains(&m.id)).count();
                vec![
                    Line::from(c.name.as_str()).bold(),
                    Line::from(c.description.as_str()),
                    Line::from(format!("{picked} of {total} selected")).dim(),
                    Line::from(""),
                    Line::from("Tab or → to pick modules · Enter to review all changes").dim(),
                ]
            })
        };
        let list = List::new(items)
            .block(Block::bordered().title(" Modules ").border_style(focused(self.focus_modules)))
            .highlight_style(if self.focus_modules { Style::new().bg(Color::DarkGray) } else { Style::new() });
        f.render_stateful_widget(list, right, &mut self.module);
        f.render_widget(
            Paragraph::new(detail.unwrap_or_default())
                .wrap(Wrap { trim: true })
                .scroll((self.detail_scroll, 0))
                .block(Block::bordered().title_bottom(" PgUp/PgDn ")),
            info,
        );
    }
}
