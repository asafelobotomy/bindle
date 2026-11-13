//! "What's new" in the terminal: every item grouped (new, changed, retired,
//! removed by you, optional) with a tick box, and the selected item's why and
//! changes beside it; a change that can be left out on its own (a package, a
//! file, a service) has its own tick. Space ticks, → moves into the changes,
//! h hides an item for good, Enter goes on to the review; q is "not now"
//! (it asks again next time), s skips this version.

use anyhow::Result;
use bindle_core::whatsnew::{Item, Kind, WhatsNew};
use bindle_run::whatsnew::Picks;
use bindle_run::whatsnew_state;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

/// The groups in order, with their headings.
pub const GROUPS: [(Kind, &str); 5] = [
    (Kind::New, "New for this computer"),
    (Kind::Updated, "Changed since it was set up"),
    (Kind::Retired, "No longer in Bindle (clean-up)"),
    (Kind::Removed, "You took these out of your setup (clean-up)"),
    (Kind::Optional, "New in this version (optional)"),
];

/// How the page was left.
pub enum Ended {
    Apply(Picks),
    /// "Not now": nothing stored, it asks again next time.
    Later,
    /// Done with this version: it opens by itself again for a newer one.
    Skip,
}

/// Rows: a heading (None) or an item (index into `items`).
fn rows(wn: &WhatsNew, hidden: &[usize]) -> Vec<(Option<usize>, String)> {
    let mut out = vec![];
    for (kind, title) in GROUPS {
        let idx: Vec<usize> =
            (0..wn.items.len()).filter(|i| wn.items[*i].kind == kind && !hidden.contains(i)).collect();
        if idx.is_empty() {
            continue;
        }
        out.push((None, format!("{title} ({})", idx.len())));
        out.extend(idx.into_iter().map(|i| (Some(i), wn.items[i].name.clone())));
    }
    out
}

struct Screen<'a> {
    wn: &'a WhatsNew,
    hidden: Vec<usize>,
    rows: Vec<(Option<usize>, String)>,
    list: ListState,
    picks: Picks,
    /// In the changes: the index of the keyed change under the cursor.
    change: Option<usize>,
    scroll: u16,
}

pub fn run(wn: &WhatsNew, picks: Picks) -> Result<Ended> {
    let rows = rows(wn, &[]);
    let first = rows.iter().position(|(i, _)| i.is_some());
    let list = ListState::default().with_selected(first);
    let mut s = Screen { wn, hidden: vec![], rows, list, picks, change: None, scroll: 0 };
    let mut terminal = ratatui::init();
    let done = s.event_loop(&mut terminal);
    ratatui::restore();
    done
}

impl Screen<'_> {
    fn event_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<Ended> {
        loop {
            terminal.draw(|f| self.draw(f))?;
            let Event::Key(k) = event::read()? else { continue };
            if k.kind != KeyEventKind::Press {
                continue;
            }
            match (k.code, self.change) {
                (KeyCode::Char('q') | KeyCode::Esc, None) => return Ok(Ended::Later),
                (KeyCode::Char('s'), _) => return Ok(Ended::Skip),
                (KeyCode::Enter, _) => return Ok(Ended::Apply(self.picks.clone())),
                (KeyCode::Right | KeyCode::Tab, None) => self.change = (!self.keyed().is_empty()).then_some(0),
                (KeyCode::Left | KeyCode::Esc | KeyCode::Tab, Some(_)) => self.change = None,
                (KeyCode::Up, Some(c)) => self.change = Some(c.saturating_sub(1)),
                (KeyCode::Down, Some(c)) => self.change = Some((c + 1).min(self.keyed().len().saturating_sub(1))),
                (KeyCode::Up, None) => self.step(-1),
                (KeyCode::Down, None) => self.step(1),
                (KeyCode::PageDown, _) => self.scroll = self.scroll.saturating_add(5),
                (KeyCode::PageUp, _) => self.scroll = self.scroll.saturating_sub(5),
                (KeyCode::Char(' '), c) => self.toggle(c),
                (KeyCode::Char('h'), None) => self.hide()?,
                _ => {}
            }
        }
    }

    fn toggle(&mut self, change: Option<usize>) {
        let Some(item) = self.current() else { return };
        let id = item.id.clone();
        match change.and_then(|c| self.keyed().get(c).cloned()) {
            Some(key) => self.picks.toggle_change(&id, &key),
            None => self.picks.toggle_item(&id),
        }
    }

    /// Hide the current item for good (until a later Bindle changes it again).
    fn hide(&mut self) -> Result<()> {
        let Some(r) = self.list.selected() else { return Ok(()) };
        let Some(i) = self.rows[r].0 else { return Ok(()) };
        whatsnew_state::hide(&self.wn.items[i])?;
        self.picks.items.remove(&self.wn.items[i].id);
        self.hidden.push(i);
        self.rows = rows(self.wn, &self.hidden);
        let next = self.rows.iter().position(|(i, _)| i.is_some());
        self.list.select(next);
        Ok(())
    }

    /// The keys of the current item's changes that can be left out alone.
    fn keyed(&self) -> Vec<String> {
        self.current().map(|i| i.changes.iter().filter_map(|c| c.key.clone()).collect()).unwrap_or_default()
    }

    /// Move to the next item (headings are skipped).
    fn step(&mut self, by: i32) {
        let mut at = self.list.selected().unwrap_or(0) as i32;
        loop {
            at += by;
            if at < 0 || at as usize >= self.rows.len() {
                return;
            }
            if self.rows[at as usize].0.is_some() {
                self.list.select(Some(at as usize));
                self.scroll = 0;
                return;
            }
        }
    }

    fn current(&self) -> Option<&Item> {
        self.list.selected().and_then(|r| self.rows.get(r)).and_then(|r| r.0).map(|i| &self.wn.items[i])
    }

    fn draw(&mut self, f: &mut Frame) {
        let [top, body, keys] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(5), Constraint::Length(1)]).areas(f.area());
        let since = self.wn.since.as_deref().map_or("your earlier setup".to_string(), |v| format!("Bindle {v}"));
        let mut head = vec![Line::from(format!("What's new in Bindle {} since {since}", self.wn.version)).bold()];
        head.push(if self.wn.recorded {
            Line::from("Tick what to apply; nothing changes until you confirm the review.").dim()
        } else {
            Line::from("No record from an older Bindle: compared with what is installed.").yellow()
        });
        f.render_widget(Paragraph::new(head), top);
        let [left, right] = Layout::horizontal([Constraint::Percentage(40), Constraint::Min(30)]).areas(body);
        let items: Vec<ListItem> = self
            .rows
            .iter()
            .map(|(i, text)| match i {
                None => ListItem::new(Line::from(text.as_str()).bold().fg(Color::Yellow)),
                Some(i) => {
                    let tick = if self.picks.items.contains(&self.wn.items[*i].id) { "[x] " } else { "[ ] " };
                    ListItem::new(format!("  {tick}{text}"))
                }
            })
            .collect();
        let focus = |on: bool| if on { Style::new().fg(Color::Yellow) } else { Style::new() };
        let title = format!(" {} picked ", self.picks.items.len());
        let list = List::new(items)
            .block(Block::bordered().title(title).border_style(focus(self.change.is_none())))
            .highlight_style(Style::new().bg(Color::DarkGray));
        f.render_stateful_widget(list, left, &mut self.list);
        let detail = self.current().map(|i| detail(i, &self.picks, self.change)).unwrap_or_default();
        let block = Block::bordered().title(" Details ").border_style(focus(self.change.is_some()));
        f.render_widget(Paragraph::new(detail).block(block).wrap(Wrap { trim: false }).scroll((self.scroll, 0)), right);
        let k = if self.change.is_some() {
            "↑↓ change   Space tick it   ← back   Enter review and apply   s skip this version"
        } else {
            "↑↓ move   Space tick   → its changes   h hide   Enter review and apply   q not now   s skip this version"
        };
        f.render_widget(Paragraph::new(k).dim(), keys);
    }
}

/// An item's why and changes: diff lines coloured, keyed changes with
/// their own tick (the one under the cursor highlighted).
fn detail(item: &Item, picks: &Picks, cursor: Option<usize>) -> Vec<Line<'static>> {
    let mut out = vec![Line::from(item.name.clone()).bold(), Line::from(item.why.clone()).dim(), Line::from("")];
    let mut n = 0;
    for c in &item.changes {
        let line = match &c.key {
            Some(key) => {
                let tick = if picks.change_picked(&item.id, key) { "[x] " } else { "[ ] " };
                let style = if cursor == Some(n) { Style::new().bg(Color::DarkGray) } else { Style::new() };
                n += 1;
                Line::from(Span::styled(format!("{tick}{}", c.text), style))
            }
            None => Line::from(format!("    {}", c.text)),
        };
        out.push(line);
        for d in &c.diff {
            let style = match d.chars().next() {
                Some('+') => Style::new().fg(Color::Green),
                Some('-') => Style::new().fg(Color::Red),
                _ => Style::new().add_modifier(Modifier::DIM),
            };
            out.push(Line::from(Span::styled(format!("      {d}"), style)));
        }
    }
    if item.changes.is_empty() {
        out.push(Line::from("Nothing changes until it is ticked and applied.").dim());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, kind: Kind) -> Item {
        Item { id: id.into(), name: id.into(), kind, why: String::new(), ticked: true, changes: vec![] }
    }

    #[test]
    fn rows_group_items_under_headings_without_hidden_ones() {
        let wn = WhatsNew {
            items: vec![item("a", Kind::Updated), item("b", Kind::New), item("c", Kind::Updated)],
            ..Default::default()
        };
        let r = rows(&wn, &[]);
        let text: Vec<&str> = r.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(text, ["New for this computer (1)", "b", "Changed since it was set up (2)", "a", "c"]);
        assert_eq!(r[1].0, Some(1));
        let r = rows(&wn, &[1]);
        assert_eq!(r[0].1, "Changed since it was set up (2)");
    }
}
