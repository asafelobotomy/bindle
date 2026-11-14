//! The Install screen of `bindle install`: one progress view for the whole
//! install (archinstall, then Bindle's setup in the new system), then
//! Restart. The command runs in its own pseudo-terminal (`script`, from
//! util-linux, which the ISO has), so it behaves as on a console while its
//! output is read here: the steps on the left, the latest lines on the right.
//! Everything it prints is also kept in a log file.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::apply::sys::shell_words;
use crate::livesys::strip_ansi;

/// The steps shown, with the output line that starts each (archinstall's
/// own messages, and the plugin's `[bindle]` line for the last).
const PHASES: [(&str, &[&str]); 6] = [
    ("Getting ready", &[]),
    ("Disk", &["Creating partitions", "Starting device modifications"]),
    ("Arch base system", &["Installing packages:"]),
    ("Boot loader", &["Adding bootloader"]),
    ("Users and desktop", &["Creating user "]),
    ("Your apps and setup", &["[bindle] Setting up the new system"]),
];
const KEEP_LINES: usize = 400;

/// How the install ended, for the last screen.
#[derive(Debug, Clone, PartialEq)]
pub enum End {
    /// Everything is set up: restart into the finished desktop.
    Ready,
    /// Arch is installed and boots; Bindle finishes the rest at first login.
    Unfinished(String),
    /// Nothing usable was installed.
    Failed(String),
}

pub struct Progress {
    pub title: String,
    phase: usize,
    /// `[3/16] write …` from Bindle's apply in the new system.
    step: Option<String>,
    lines: VecDeque<String>,
    partial: String,
    started: Instant,
    pub end: Option<End>,
    pub log: String,
}

impl Progress {
    pub fn new(title: String, log: String) -> Self {
        Progress {
            title,
            phase: 0,
            step: None,
            lines: VecDeque::new(),
            partial: String::new(),
            started: Instant::now(),
            end: None,
            log,
        }
    }

    /// Output as it arrives: split into lines (`\r` redraws count as lines).
    pub fn feed(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes).into_owned();
        for ch in text.chars() {
            if ch == '\n' || ch == '\r' {
                let line = std::mem::take(&mut self.partial);
                self.line(&line);
            } else {
                self.partial.push(ch);
            }
        }
    }

    fn line(&mut self, raw: &str) {
        let line = strip_ansi(raw);
        let line = line.trim_end();
        if line.trim().is_empty() {
            return;
        }
        // Steps only move forward: later output can repeat earlier words.
        for (i, (_, starts)) in PHASES.iter().enumerate().skip(self.phase + 1) {
            if starts.iter().any(|s| line.trim_start().starts_with(s)) {
                self.phase = i;
                self.step = None;
            }
        }
        let t = line.trim_start();
        if self.phase == PHASES.len() - 1
            && let Some((count, what)) = t.strip_prefix('[').and_then(|r| r.split_once("] "))
            && let Some((n, of)) = count.split_once('/')
            && n.parse::<u32>().is_ok()
            && of.parse::<u32>().is_ok()
        {
            self.step = Some(format!("Step {n} of {of}: {what}"));
        }
        self.lines.push_back(line.to_string());
        if self.lines.len() > KEEP_LINES {
            self.lines.pop_front();
        }
    }

    pub fn last_lines(&self, n: usize) -> Vec<String> {
        self.lines.iter().skip(self.lines.len().saturating_sub(n)).cloned().collect()
    }

    pub fn draw(&self, f: &mut Frame) {
        let [head, body, msg, foot] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1), Constraint::Length(1)])
                .areas(f.area());
        let secs = self.started.elapsed().as_secs();
        let head_line = Line::from(vec![
            Span::from(" Bindle install  ").bold().fg(Color::Yellow),
            Span::from(self.title.clone()),
            Span::from(format!("   {}:{:02}", secs / 60, secs % 60)).dim(),
        ]);
        f.render_widget(head_line, head);
        let (left, right) = if body.width >= 100 {
            let [l, r] = Layout::horizontal([Constraint::Length(30), Constraint::Min(30)]).areas(body);
            (l, r)
        } else {
            let [l, r] =
                Layout::vertical([Constraint::Length(PHASES.len() as u16 + 2), Constraint::Min(0)]).areas(body);
            (l, r)
        };
        let done = matches!(self.end, Some(End::Ready | End::Unfinished(_)));
        let spin = ["⠋", "⠙", "⠸", "⠴", "⠦", "⠇"][(self.started.elapsed().as_millis() / 150) as usize % 6];
        let steps: Vec<Line> = PHASES
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                if i < self.phase || (done && self.end == Some(End::Ready)) {
                    Line::from(format!(" ✓ {name}")).fg(Color::Green)
                } else if i == self.phase && self.end.is_none() {
                    Line::from(format!(" {spin} {name}")).bold().fg(Color::Yellow)
                } else if i == self.phase {
                    Line::from(format!(" ✗ {name}")).fg(Color::Red)
                } else {
                    Line::from(format!("   {name}")).dim()
                }
            })
            .collect();
        f.render_widget(Paragraph::new(steps).block(Block::bordered().title(" Steps ")), left);
        let room = right.height.saturating_sub(2) as usize;
        let width = right.width.saturating_sub(2) as usize;
        let tail: Vec<Line> =
            self.last_lines(room).into_iter().map(|l| Line::from(l.chars().take(width).collect::<String>())).collect();
        f.render_widget(Paragraph::new(tail).block(Block::bordered().title(" What's happening ")), right);
        let (status, keys) = match &self.end {
            None => (
                Line::from(format!(" {}", self.step.clone().unwrap_or_else(|| format!("{}…", PHASES[self.phase].0)))),
                " This takes a while. Don't switch the computer off.".to_string(),
            ),
            Some(End::Ready) => (
                Line::from(" Done. Your system is ready: remove the install media and restart.")
                    .bold()
                    .fg(Color::Green),
                " Enter restart now · q stay in the live system".to_string(),
            ),
            // Not restarted from here: what's left may need doing first.
            Some(End::Unfinished(why)) => (
                Line::from(format!(" {why}")).fg(Color::Yellow),
                " q leave, then restart when ready: systemctl reboot".into(),
            ),
            Some(End::Failed(why)) => (
                Line::from(format!(" {why}")).bold().fg(Color::Red),
                format!(" q leave · everything it printed: {}", self.log),
            ),
        };
        f.render_widget(status, msg);
        f.render_widget(Line::from(keys).dim(), foot);
    }
}

/// `cmd` in a pseudo-terminal of its own, its output on a pipe. None: no
/// `script` here (not the Arch ISO), so the caller runs it the plain way.
fn spawn(cmd: &Command) -> Option<std::process::Child> {
    let argv: Vec<String> =
        std::iter::once(cmd.get_program()).chain(cmd.get_args()).map(|s| s.to_string_lossy().into_owned()).collect();
    let mut c = Command::new("script");
    // The new terminal starts 0×0; give it a size before the command draws.
    let line = format!("stty rows 50 cols 160 2>/dev/null; exec {}", shell_words(&argv));
    c.args(["-qefc", &line, "/dev/null"]);
    for (k, v) in cmd.get_envs() {
        match v {
            Some(v) => c.env(k, v),
            None => c.env_remove(k),
        };
    }
    // A session of its own: when this terminal (or SSH connection) goes
    // away, its hang-up doesn't reach the install.
    // SAFETY: setsid and signal are async-signal-safe and touch no memory of this process.
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            Ok(())
        });
    }
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()
}

/// The log, kept across tries (appended, so a retry keeps the earlier
/// failure) and readable by root only: it can show the disk layout.
pub fn open_log(log: &Path) -> Option<File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut f = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(log).ok()?;
    let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
    let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let _ = writeln!(f, "\n===== bindle install started (unix time {when}) =====");
    Some(f)
}

/// Move what has arrived into the view and the log. False: the output ended.
fn drain(rx: &Receiver<Vec<u8>>, p: &mut Progress, log: &mut Option<File>) -> bool {
    loop {
        match rx.try_recv() {
            Ok(chunk) => {
                if let Some(f) = log.as_mut() {
                    let _ = f.write_all(&chunk);
                }
                p.feed(&chunk);
            }
            Err(TryRecvError::Empty) => return true,
            Err(TryRecvError::Disconnected) => return false,
        }
    }
}

fn reader(mut out: impl Read + Send + 'static) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = out.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    rx
}

pub struct Finished {
    pub end: End,
    /// Enter on the last screen: restart now.
    pub restart: bool,
    /// The last lines printed, to leave on the terminal after a failure.
    pub tail: Vec<String>,
}

/// Run the install on the progress screen. `outcome` looks at the result
/// once the command has ended (it runs before the last screen is shown).
/// None: `script` isn't available, nothing was started.
pub fn run(
    cmd: &Command,
    title: String,
    log: &Path,
    outcome: impl FnOnce(ExitStatus) -> End,
) -> Result<Option<Finished>> {
    let Some(mut child) = spawn(cmd) else { return Ok(None) };
    let rx = reader(child.stdout.take().context("the install's output")?);
    let mut file = open_log(log);
    let mut p = Progress::new(title, log.display().to_string());
    let mut outcome = Some(outcome);
    let mut terminal = ratatui::init();
    let result = (|| -> Result<bool> {
        loop {
            if p.end.is_none() && !drain(&rx, &mut p, &mut file) {
                let status = child.wait().context("wait for the install")?;
                if let Some(decide) = outcome.take() {
                    p.end = Some(decide(status));
                }
            }
            // The terminal is gone (hang-up): keep following the install
            // into the log until it ends, then leave without asking.
            if super::guard::hung_up() || terminal.draw(|f| p.draw(f)).is_err() {
                if p.end.is_some() {
                    return Ok(false);
                }
                std::thread::sleep(Duration::from_millis(150));
                continue;
            }
            if !event::poll(Duration::from_millis(150)).unwrap_or(false) {
                continue;
            }
            let Ok(Event::Key(k)) = event::read() else { continue };
            if k.kind != KeyEventKind::Press {
                continue;
            }
            match (&p.end, k.code) {
                (Some(End::Ready), KeyCode::Enter) => return Ok(true),
                (Some(_), KeyCode::Char('q') | KeyCode::Esc) => return Ok(false),
                (Some(End::Failed(_)), KeyCode::Enter) => return Ok(false),
                _ => {}
            }
        }
    })();
    ratatui::restore();
    let restart = result?;
    let end = p.end.clone().unwrap_or_else(|| End::Failed("the install was interrupted".into()));
    Ok(Some(Finished { end, restart, tail: p.last_lines(15) }))
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
