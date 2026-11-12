//! Drawing for `bindle install`, in the `bindle` wizard's style: a yellow
//! title line with the steps, bordered panes, the highlighted row on dark
//! grey with `▸`, and the keys on the bottom line.

use bindle_core::install::hardware::Setting;
use bindle_core::install::preset;
use bindle_core::install::target::Filesystem;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph, Wrap};

use super::{STEPS, Step, Wizard};

fn focused(on: bool) -> Style {
    if on { Style::new().fg(Color::Yellow) } else { Style::new() }
}

pub(super) fn list<'a>(items: Vec<ListItem<'a>>, title: &'a str, focus: bool) -> List<'a> {
    List::new(items)
        .block(Block::bordered().title(title).border_style(focused(focus)))
        .highlight_style(Style::new().bg(Color::DarkGray))
        .highlight_symbol("▸ ")
}

/// Two panes side by side, or stacked on an 80-column console.
pub(super) fn panes(body: Rect, left: u16) -> (Rect, Rect) {
    if body.width >= 100 {
        let [l, r] = Layout::horizontal([Constraint::Length(left), Constraint::Min(30)]).areas(body);
        (l, r)
    } else {
        let [l, r] = Layout::vertical([Constraint::Percentage(50), Constraint::Min(0)]).areas(body);
        (l, r)
    }
}

impl Wizard<'_> {
    pub(crate) fn draw(&mut self, f: &mut Frame) {
        let [head, body, msg, foot] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1), Constraint::Length(1)])
                .areas(f.area());
        let mut spans = vec![Span::from(" Bindle install  ").bold().fg(Color::Yellow)];
        for (i, (s, name)) in STEPS.iter().enumerate() {
            if i > 0 {
                spans.push(Span::from(" › ").dim());
            }
            spans.push(if *s == self.step {
                Span::from(*name).bold().fg(Color::Yellow)
            } else {
                Span::from(*name).dim()
            });
        }
        let fx = self.facts;
        spans.push(
            Span::from(format!(
                "   {} · {:.0} GiB · {}",
                fx.cpu_vendor,
                fx.ram_mib as f64 / 1024.0,
                if fx.laptop { "laptop" } else { "desktop PC" }
            ))
            .dim(),
        );
        f.render_widget(Line::from(spans), head);
        if let Some(m) = &self.message {
            f.render_widget(Line::from(format!(" {m}")).fg(Color::Yellow), msg);
        }
        if self.confirm {
            let disk = self.answers().map(|a| a.disk).unwrap_or_default();
            let ask = format!(" Erase everything on {disk} and install? y to start, any other key to go back");
            f.render_widget(Line::from(ask).bold().fg(Color::Red), msg);
        }
        let keys = match self.step {
            _ if self.passphrase.is_some() => "type the Wi-Fi password · Enter join · Esc cancel",
            Step::Network => "↑↓ choose · c connect · r scan again · Enter next · q quit",
            Step::Desktop => "↑↓ choose · Tab switch list · Enter next · Esc back · q quit",
            Step::Hardware => "↑↓ choose · ←→ change · r recommended again · Enter next · Esc back · q quit",
            Step::Apps => "↑↓ choose · Space tick or untick · m more apps · Enter next · Esc back · q quit",
            Step::Disk => "↑↓ choose disk · f filesystem · e encryption · Enter next · Esc back · q quit",
            Step::You => "↑↓ field · type to edit · Enter next field · Esc back",
            Step::Review => "i install · Esc back · q quit",
        };
        f.render_widget(Line::from(format!(" {keys}")).dim(), foot);
        match self.step {
            Step::Network => self.draw_network(f, body),
            Step::Desktop => self.draw_desktop(f, body),
            Step::Hardware => self.draw_hardware(f, body),
            Step::Apps => self.draw_apps(f, body),
            Step::Disk => self.draw_disk(f, body),
            Step::You => self.draw_you(f, body),
            Step::Review => self.draw_review(f, body),
        }
    }

    fn draw_network(&mut self, f: &mut Frame, body: Rect) {
        let (left, right) = panes(body, 44);
        let items: Vec<ListItem> = self
            .networks
            .iter()
            .map(|n| {
                let bars = "▂▄▆█".chars().take(n.signal as usize).collect::<String>();
                let mark = if n.connected { " ✓" } else { "" };
                ListItem::new(format!("{:<24} {:<5} {bars}{mark}", n.ssid, n.security))
            })
            .collect();
        let title = if self.wifi.is_some() { " Wi-Fi networks " } else { " Wi-Fi: no wireless device " };
        f.render_stateful_widget(list(items, title, true), left, &mut self.net);
        let mut lines = vec![if self.online {
            Line::from("✓ Connected to the internet.").fg(Color::Green)
        } else {
            Line::from("• Not connected yet.").fg(Color::Yellow)
        }];
        lines.push(Line::from(""));
        lines.push(Line::from("Bindle downloads everything during the install, so it needs a connection now."));
        lines.push(Line::from("The Wi-Fi you join here is saved in the new system: you won't be asked again."));
        lines.push(Line::from("A network cable works without any setup."));
        if let Some(p) = &self.passphrase {
            lines.push(Line::from(""));
            lines.push(Line::from(format!("Password: {}▏", "•".repeat(p.chars().count()))).bold());
        }
        let block = Block::bordered().title(" Network ");
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), right);
    }

    fn draw_desktop(&mut self, f: &mut Frame, body: Rect) {
        let (lists, right) = panes(body, 64);
        let [l, m] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(lists);
        let mut items = vec![ListItem::new("No desktop")];
        items.extend(self.desktops.iter().map(|d| match &d.broken {
            Some(_) => ListItem::new(format!("{} (unavailable)", d.name)).style(Style::new().fg(Color::DarkGray)),
            None if d.status.is_some() => ListItem::new(d.label()),
            None => ListItem::new(format!("{} ({})", d.name, d.display)),
        }));
        f.render_stateful_widget(list(items, " Desktop ", !self.focus_profile), l, &mut self.desktop);
        let items: Vec<ListItem> = self.profiles().iter().map(|p| ListItem::new(p.name.clone())).collect();
        f.render_stateful_widget(list(items, " Profile ", self.focus_profile), m, &mut self.profile);
        let choice = self.choice();
        let mut lines: Vec<Line> = Vec::new();
        if let Some(p) = self.profiles().get(self.profile.selected().unwrap_or(0)) {
            lines.push(Line::from(format!("{}: {}", p.name, p.description)));
            lines.push(Line::from(""));
        }
        lines.push(Line::from("Recommended for this hardware (the next page changes it):").bold());
        let summary = preset::summary(&preset::build(self.facts, self.desktops, &choice, None));
        lines.extend(summary.into_iter().filter(|l| !l.contains("archinstall")).map(Line::from));
        let sel = self.selection();
        if !sel.include.is_empty() {
            lines.push(Line::from(format!("Also         {}", sel.include.join(", "))));
        }
        let block = Block::bordered().title(" What gets installed ");
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), right);
    }

    fn draw_disk(&mut self, f: &mut Frame, body: Rect) {
        let (left, right) = panes(body, 56);
        let items: Vec<ListItem> = self.disks.iter().map(|d| ListItem::new(d.label())).collect();
        f.render_stateful_widget(list(items, " Install to ", true), left, &mut self.disk);
        let fs = match self.filesystem {
            Filesystem::Btrfs => "Btrfs, with snapshots you can roll back to (Snapper)",
            Filesystem::Ext4 => "ext4, no snapshots",
        };
        let enc = if self.encrypt { "on: asks for a passphrase at every boot" } else { "off" };
        let hw = self.hardware();
        let boot = hw.rows.iter().find(|r| r.setting == Setting::Bootloader).map_or("", |r| r.choice().label);
        let boot = format!("{boot} ({})", if self.uefi { "UEFI" } else { "BIOS" });
        let mut lines = vec![
            Line::from(format!("f  Filesystem   {fs}")),
            Line::from(format!("e  Encryption   {enc}")),
            Line::from(format!("   Boot loader  {boot}")),
            Line::from("   Layout       1 GiB boot partition, the rest for the system"),
            Line::from(""),
        ];
        if let Some(d) = self.disks.get(self.disk.selected().unwrap_or(0)) {
            lines.push(Line::from(format!("Everything on {} will be erased.", d.path)).fg(Color::Red));
        } else {
            lines.push(Line::from("No disk found to install to.").fg(Color::Red));
        }
        let block = Block::bordered().title(" Disk ");
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), right);
    }

    fn draw_you(&mut self, f: &mut Frame, body: Rect) {
        let lines: Vec<Line> = self
            .visible()
            .into_iter()
            .map(|i| {
                let fl = &self.fields[i];
                let shown = if fl.secret { "•".repeat(fl.value.chars().count()) } else { fl.value.clone() };
                let cursor = if i == self.field { "▏" } else { "" };
                let row = format!("{} {:<16} {shown}{cursor}", if i == self.field { "▸" } else { " " }, fl.label);
                if i == self.field { Line::from(row).bg(Color::DarkGray) } else { Line::from(row) }
            })
            .collect();
        let block = Block::bordered().title(" You ").border_style(focused(true));
        f.render_widget(Paragraph::new(lines).block(block), body);
    }

    fn draw_review(&mut self, f: &mut Frame, body: Rect) {
        let choice = self.choice();
        let sel = self.selection();
        let mut lines: Vec<Line> = Vec::new();
        let desktop = choice.desktop.as_deref().unwrap_or("none");
        lines.push(Line::from(format!("Desktop      {desktop} · profile {}", choice.profile)));
        let mods = self.app_modules();
        let on = mods.iter().filter(|m| !sel.exclude.contains(&m.id)).count();
        let custom = if self.apps.is_some() { " (your choice)" } else { "" };
        lines.push(Line::from(format!("Apps         {on} of {}{custom}", mods.len())));
        match self.answers() {
            Ok(a) => {
                let fs = if a.filesystem == Filesystem::Btrfs { "Btrfs + snapshots" } else { "ext4" };
                let enc = if a.encryption.is_some() { ", encrypted" } else { "" };
                lines.push(Line::from(format!("Disk         {} (erased) · {fs}{enc}", a.disk)));
                lines.push(Line::from(format!("You          {} on {}", a.username, a.hostname)));
                lines.push(Line::from(format!("Locale       {} · {} keyboard · {}", a.locale, a.keyboard, a.timezone)));
            }
            Err(e) => lines.push(Line::from(e).fg(Color::Red)),
        }
        for r in &self.hardware().rows {
            let mark = if r.selected == r.recommended { "" } else { " (changed)" };
            lines.push(Line::from(format!(
                "{:<12} {}{mark}",
                r.label.split(' ').next().unwrap_or(r.label),
                r.choice().label
            )));
        }
        // The rest of the preset: what no page changes.
        let fixed = ["Network", "pacman", "Firewall", "Fonts", "Repos"];
        let summary = preset::summary(&preset::build(self.facts, self.desktops, &choice, None));
        lines.extend(summary.into_iter().filter(|l| fixed.iter().any(|p| l.starts_with(p))).map(Line::from));
        lines.push(Line::from(""));
        lines.push(Line::from("Bindle installs Arch with archinstall, then your desktop and apps, all before the"));
        lines.push(Line::from("first reboot. It can take a while; the screen shows each step."));
        let block = Block::bordered().title(" Review: nothing changes until you press i ");
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), body);
    }
}
