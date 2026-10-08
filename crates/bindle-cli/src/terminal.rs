//! Open a command in a terminal emulator (for autostart, where none exists).

use std::env;
use std::process::Command;

use anyhow::{Result, bail};

#[derive(Clone, Copy)]
enum Style {
    /// `term cmd args…`
    Direct,
    /// `term -- cmd args…`
    Dash,
    /// `term -e cmd args…`
    E,
    /// `term -x cmd args…`
    X,
    /// `wezterm start -- cmd args…`
    Wez,
}

const TERMINALS: &[(&str, Style)] = &[
    ("xdg-terminal-exec", Style::Direct),
    ("kgx", Style::Dash),
    ("ptyxis", Style::Dash),
    ("gnome-terminal", Style::Dash),
    ("konsole", Style::E),
    ("cosmic-term", Style::E),
    ("xfce4-terminal", Style::X),
    ("mate-terminal", Style::X),
    ("qterminal", Style::E),
    ("deepin-terminal", Style::E),
    ("terminology", Style::E),
    ("alacritty", Style::E),
    ("kitty", Style::Direct),
    ("foot", Style::Direct),
    ("wezterm", Style::Wez),
    ("urxvt", Style::E),
    ("xterm", Style::E),
];

/// Desktop-native terminal first, so autostart opens what the user expects.
fn preferred() -> Vec<&'static str> {
    let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
    let native: &[&str] = if desktop.contains("gnome") {
        &["kgx", "ptyxis", "gnome-terminal"]
    } else if desktop.contains("kde") {
        &["konsole"]
    } else if desktop.contains("cosmic") {
        &["cosmic-term"]
    } else if desktop.contains("xfce") {
        &["xfce4-terminal"]
    } else if desktop.contains("mate") {
        &["mate-terminal"]
    } else if desktop.contains("lxqt") {
        &["qterminal"]
    } else {
        &[]
    };
    native.to_vec()
}

fn on_path(prog: &str) -> bool {
    env::var_os("PATH").is_some_and(|p| env::split_paths(&p).any(|d| d.join(prog).is_file()))
}

pub fn spawn(argv: &[String]) -> Result<()> {
    let from_env = env::var("TERMINAL").ok();
    let mut order: Vec<&str> = from_env.iter().map(String::as_str).collect();
    order.extend(preferred());
    order.extend(TERMINALS.iter().map(|(t, _)| *t));
    for term in order {
        if !on_path(term) {
            continue;
        }
        let style = TERMINALS.iter().find(|(t, _)| *t == term).map_or(Style::E, |(_, s)| *s);
        let mut cmd = Command::new(term);
        // xterm/urxvt default to bitmap fonts without ✓ • ↑↓; ask for a scalable
        // monospace one (Bindle's preset installs DejaVu and Noto).
        match term {
            "xterm" => {
                cmd.args(["-fa", "Monospace", "-fs", "11"]);
            }
            "urxvt" => {
                cmd.args(["-fn", "xft:Monospace:size=11"]);
            }
            _ => {}
        }
        match style {
            Style::Direct => {}
            Style::Dash => {
                cmd.arg("--");
            }
            Style::E => {
                cmd.arg("-e");
            }
            Style::X => {
                cmd.arg("-x");
            }
            Style::Wez => {
                cmd.args(["start", "--"]);
            }
        }
        if cmd.args(argv).spawn().is_ok() {
            return Ok(());
        }
    }
    bail!("no terminal emulator found; run `bindle` from a terminal")
}
