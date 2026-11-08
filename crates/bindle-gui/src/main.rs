//! Bindle's desktop wizard: a Kirigami window over the same catalog, planner
//! and engine as the terminal wizard. On uQ it takes its colours from uQ's
//! theme tokens; elsewhere it follows the desktop's Qt theme.
//!
//! `bindle-gui --screenshots <dir>` renders every screen offscreen into `dir`
//! (with `BINDLE_UQ_TOKENS=<tokens.json>` to pick a uQ theme) and exits.

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

mod app;
mod backend;
mod model;
mod theme;

/// The directory given with `--screenshots`, if any.
pub fn screenshot_dir() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == "--screenshots").and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    if screenshot_dir().is_some() && std::env::var_os("QT_QPA_PLATFORM").is_none() {
        // SAFETY: single-threaded here, before Qt or any other thread starts.
        unsafe { std::env::set_var("QT_QPA_PLATFORM", "offscreen") };
    }
    cxx_qt::init_qml_module!("org.bindle.wizard");

    // Plasma provides the native Kirigami style; elsewhere Fusion, which takes
    // its colours from the window's palette (set from uQ's tokens on uQ).
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        let on_kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|p| p == "KDE"));
        let uq = theme::tokens_path().is_some_and(|p| p.exists());
        let style = if on_kde && !uq { "org.kde.desktop" } else { "Fusion" };
        QQuickStyle::set_style(&QString::from(style));
    }

    let mut app = QGuiApplication::new();
    app::ffi::bindle_describe_application(env!("CARGO_PKG_VERSION"));
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/org/bindle/wizard/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        std::process::exit(app.exec());
    }
}
