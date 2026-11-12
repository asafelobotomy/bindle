//! The Install screen of `bindle install`: one progress view for the whole
//! install (archinstall, then Bindle's setup in the new system), then
//! Restart. The command runs in its own pseudo-terminal (`script`, from
//! util-linux, which the ISO has), so it behaves as on a console while its
//! output is read here: the steps on the left, the latest lines on the right.
//! Everything it prints is also kept in a log file.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
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
            Some(End::Unfinished(why)) => (
                Line::from(format!(" {why}")).fg(Color::Yellow),
                " Enter restart now · q stay in the live system".into(),
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
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()
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
    let mut file = std::fs::OpenOptions::new().create(true).write(true).truncate(true).open(log).ok();
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
            terminal.draw(|f| p.draw(f))?;
            if !event::poll(Duration::from_millis(150))? {
                continue;
            }
            let Event::Key(k) = event::read()? else { continue };
            if k.kind != KeyEventKind::Press {
                continue;
            }
            match (&p.end, k.code) {
                (Some(End::Ready | End::Unfinished(_)), KeyCode::Enter) => return Ok(true),
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
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn screen(p: &Progress, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| p.draw(f)).unwrap();
        term.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn follows_the_install_through_its_steps() {
        let mut p = Progress::new("to /dev/vda".into(), "/var/log/x.log".into());
        assert_eq!(p.phase, 0);
        p.feed(b"Creating partitions: /dev/vda\nWaiting for time sync\n");
        assert_eq!(p.phase, 1);
        // Split across reads, with colour codes and a carriage-return redraw.
        p.feed(b"\x1b[1mInstalling pack");
        p.feed(b"ages: ['base']\x1b[0m\r\n downloading 10%\r downloading 90%\n");
        assert_eq!(p.phase, 2);
        assert_eq!(p.last_lines(2), [" downloading 10%", " downloading 90%"]);
        p.feed(b"Adding bootloader Systemd-boot to /dev/vda1\nCreating user solon\n");
        assert_eq!(p.phase, 4);
        // Earlier words later on don't move it back; counters only count in the last step.
        p.feed(b"Creating partitions: again\n[1/3] not yet\n");
        assert_eq!((p.phase, p.step.clone()), (4, None));
        p.feed(b"[bindle] Setting up the new system for solon (this can take a while: AUR builds)\xe2\x80\xa6\n");
        p.feed(b"[3/16] write /etc/sysctl.d/99-bindle-zram.conf [zram]\n");
        assert_eq!(p.phase, 5);
        assert_eq!(p.step.as_deref(), Some("Step 3 of 16: write /etc/sysctl.d/99-bindle-zram.conf [zram]"));
        let s = screen(&p, 120, 30);
        assert!(s.contains("Step 3 of 16") && s.contains("✓ Boot loader") && s.contains("to /dev/vda"), "{s}");
    }

    #[test]
    fn last_screens() {
        let mut p = Progress::new("to /dev/vda".into(), "/var/log/x.log".into());
        p.feed(b"Installing packages: ['base']\nerror: failed retrieving file\n");
        p.end = Some(End::Failed("archinstall stopped (exit status: 1).".into()));
        for (w, h) in [(80, 25), (140, 45)] {
            let s = screen(&p, w, h);
            assert!(
                s.contains("✗ Arch base system") && s.contains("/var/log/x.log") && s.contains("failed retrieving")
            );
        }
        p.end = Some(End::Ready);
        let s = screen(&p, 80, 25);
        assert!(s.contains("Enter restart now") && s.contains("✓ Your apps and setup"));
        p.end = Some(End::Unfinished("Bindle opens when you first log in and finishes the rest.".into()));
        assert!(screen(&p, 80, 25).contains("finishes the rest"));
    }

    #[test]
    fn output_reaches_the_view_and_the_log() {
        let dir = std::env::temp_dir().join(format!("bindle-progress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("install.log");
        let mut log = File::create(&path).ok();
        let mut child = Command::new("sh")
            .args(["-c", "echo 'Creating partitions: /dev/vda'; echo 'Installing packages: x'"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let rx = reader(child.stdout.take().unwrap());
        let mut p = Progress::new(String::new(), String::new());
        while drain(&rx, &mut p, &mut log) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(child.wait().unwrap().success());
        assert_eq!(p.phase, 2);
        drop(log);
        assert!(std::fs::read_to_string(&path).unwrap().contains("Installing packages: x"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
