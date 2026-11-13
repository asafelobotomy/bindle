//! Bindle's desktop wizard: a Kirigami window over the same catalog, planner
//! and engine as the terminal wizard. On uQ it takes its colours from uQ's
//! theme tokens; elsewhere it follows the desktop's Qt theme.
//!
//! `bindle-gui --upgrade` is the system update window instead (what
//! `bindleupgrade --gui` opens): the same look and parts over bindleupgrade's
//! plan, from `UpgradeMain.qml` and `UpgradeBackend`.
//!
//! `bindle-gui --screenshots <dir>` renders every screen offscreen into `dir`
//! (with `BINDLE_UQ_TOKENS=<tokens.json>` to pick a uQ theme) and exits.
//!
//! Exit codes, read by `bindle first-login run` (`bindle-cli/src/gui.rs`):
//! 0 a plan was applied, 3 closed without applying, 4 no window could be
//! created (the caller falls back to the terminal wizard).

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

mod app;
mod apply;

const EXIT_APPLIED: i32 = 0;
const EXIT_CLOSED: i32 = 3;
const EXIT_NO_WINDOW: i32 = 4;
mod backend;
mod glyph;
mod model;
mod theme;
mod upgrade;
mod upgrade_backend;

/// The value after `flag` on the command line: `--screenshots <dir>`, and
/// the hidden `--sandbox <dir>` (apply records commands there, as the CLI's).
pub fn arg(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

/// A flag with no value (`--upgrade`, `--dry-run`).
pub fn flag(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

fn main() {
    if arg("--screenshots").is_some() && std::env::var_os("QT_QPA_PLATFORM").is_none() {
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
    app::ffi::bindle_set_window_icon(include_bytes!("../../../branding/bindle.svg"));
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        let main = if flag("--upgrade") { "UpgradeMain" } else { "Main" };
        engine.load(&QUrl::from(&format!("qrc:/qt/qml/org/bindle/wizard/qml/{main}.qml")));
    }
    if !app::ffi::bindle_has_window() {
        eprintln!("bindle-gui: the window couldn't be created (is Kirigami installed?)");
        std::process::exit(EXIT_NO_WINDOW);
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
    let applied = apply::APPLIED.load(std::sync::atomic::Ordering::SeqCst);
    // A screenshot run finishing is success, applied or not.
    // The update window reports its results itself; closing it is no error.
    let shots = arg("--screenshots").is_some() || flag("--upgrade");
    std::process::exit(if applied || shots { EXIT_APPLIED } else { EXIT_CLOSED });
}
