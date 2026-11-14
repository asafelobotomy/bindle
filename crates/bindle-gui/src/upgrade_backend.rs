//! The QML-facing `UpgradeBackend`: a thin bridge over
//! [`crate::upgrade::Upgrade`] for the update window (`bindle-gui
//! --upgrade`, `UpgradeMain.qml`). The checks and the run happen on worker
//! threads; pages read JSON properties.

use std::path::PathBuf;
use std::pin::Pin;

use bindle_core::Catalog;
use bindle_upgrade::plan::after_plans;
use bindle_upgrade::{prefs, probe};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use serde_json::json;

use crate::theme;
use crate::upgrade::run::{How, RunState};
use crate::upgrade::{Upgrade, sample};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// uQ's theme tokens, or `{"present": false}` off uQ.
        #[qproperty(QString, theme_json)]
        /// The Check page: `{"state": "checking", "notes"}`, then
        /// `Upgrade::check`, or `{"state": "error", "error"}`.
        #[qproperty(QString, check_json)]
        /// The Update page: `crate::upgrade::run::RunState`.
        #[qproperty(QString, run_json)]
        /// A one-line message for the footer (a terminal couldn't open).
        #[qproperty(QString, notice)]
        /// `--screenshots <dir>`: QML walks the pages and saves each here.
        #[qproperty(QString, screenshot_dir)]
        /// `--sandbox <dir>`: root commands are recorded under it.
        #[qproperty(QString, sandbox)]
        /// `--dry-run`: every command printed, none run.
        #[qproperty(bool, dry_run)]
        type UpgradeBackend = super::UpgradeRust;

        /// Look at the system (or, for screenshots, the sample).
        #[qinvokable]
        fn start_check(self: Pin<&mut UpgradeBackend>);
        /// Flip a stage's switch on the Check page.
        #[qinvokable]
        fn toggle(self: Pin<&mut UpgradeBackend>, id: &QString);
        #[qinvokable]
        fn start_run(self: Pin<&mut UpgradeBackend>);
        /// bindleupgrade in a terminal: the AUR only, or everything (after a
        /// failed system upgrade, or with no polkit agent).
        #[qinvokable]
        fn open_terminal(self: Pin<&mut UpgradeBackend>, only_aur: bool);
        #[qinvokable]
        fn reboot(self: Pin<&mut UpgradeBackend>);
        /// Open Bindle's What's new page (new and changed features to review).
        #[qinvokable]
        fn open_bindle(self: Pin<&mut UpgradeBackend>);
        /// Bindle replaced itself: open the new update window, which goes on.
        #[qinvokable]
        fn reopen(self: Pin<&mut UpgradeBackend>);
        /// Screenshots: check another sample system (`held`).
        #[qinvokable]
        fn load_sample(self: Pin<&mut UpgradeBackend>, name: &QString);
        #[qinvokable]
        fn grab_window(self: &UpgradeBackend, path: &QString) -> bool;
        #[qinvokable]
        fn glyph(self: &UpgradeBackend, name: &QString, color: &QString) -> QString;
    }

    impl cxx_qt::Threading for UpgradeBackend {}
    impl cxx_qt::Initialize for UpgradeBackend {}
}

use qobject::UpgradeBackend;

#[derive(Default)]
pub struct UpgradeRust {
    theme_json: QString,
    check_json: QString,
    run_json: QString,
    notice: QString,
    screenshot_dir: QString,
    sandbox: QString,
    dry_run: bool,
    upgrade: Option<Upgrade>,
    busy: bool,
}

fn qs(v: &serde_json::Value) -> QString {
    QString::from(&v.to_string())
}

impl cxx_qt::Initialize for UpgradeBackend {
    fn initialize(mut self: Pin<&mut Self>) {
        if let Some(dir) = crate::arg("--screenshots") {
            self.as_mut().set_screenshot_dir(QString::from(&dir));
        }
        if let Some(dir) = crate::arg("--sandbox") {
            self.as_mut().set_sandbox(QString::from(&dir));
        }
        self.as_mut().set_dry_run(crate::flag("--dry-run") || crate::flag("-n"));
        // The screenshot run shows the Checking page part-way through.
        let notes: Vec<&str> = if self.screenshots() { sample::NOTES[..3].to_vec() } else { vec![] };
        self.as_mut().set_check_json(qs(&json!({ "state": "checking", "notes": notes })));
        self.as_mut().set_run_json(qs(&json!({ "state": "idle" })));
        let path = theme::tokens_path();
        self.as_mut().set_theme_json(qs(&theme::load(path.as_ref())));
        let qt = self.qt_thread();
        theme::watch(path, move |tokens| qt.queue(move |b| b.set_theme_json(QString::from(&tokens))).is_ok());
    }
}

impl UpgradeBackend {
    fn screenshots(&self) -> bool {
        !self.rust().screenshot_dir.is_empty()
    }

    fn sandbox_dir(&self) -> Option<String> {
        let s = self.rust().sandbox.to_string();
        (!s.is_empty()).then_some(s)
    }

    fn start_check(mut self: Pin<&mut Self>) {
        if self.rust().busy {
            return;
        }
        if self.screenshots() {
            let notes: Vec<&str> = sample::NOTES.to_vec();
            self.as_mut().set_check_json(qs(&json!({ "state": "checking", "notes": notes })));
            let u = Upgrade::new(sample::probe(), prefs::Prefs::default());
            self.as_mut().set_check_json(qs(&u.check()));
            self.as_mut().rust_mut().get_mut().upgrade = Some(u);
            return;
        }
        self.as_mut().rust_mut().get_mut().busy = true;
        self.as_mut().set_check_json(qs(&json!({ "state": "checking", "notes": [] })));
        let qt = self.qt_thread();
        std::thread::Builder::new()
            .name("check".into())
            .spawn(move || {
                let fail = |e: String| {
                    let text = json!({ "state": "error", "error": e }).to_string();
                    let _ = qt.queue(move |mut b| {
                        b.as_mut().rust_mut().get_mut().busy = false;
                        b.set_check_json(QString::from(&text));
                    });
                };
                if let Some((what, todo)) = bindle_run::net::problem() {
                    return fail(format!("{what} {todo}"));
                }
                let catalog = match Catalog::arch() {
                    Ok(c) => c,
                    Err(e) => return fail(format!("{e:#}")),
                };
                let mut warnings = vec![];
                let prefs = prefs::load(&mut |w| warnings.push(w.to_string()));
                let mut notes: Vec<String> = warnings;
                let exe = crate::apply::helper_exe().unwrap_or_else(|| PathBuf::from("bindle"));
                // The Check page only looks: fwupd's list isn't refreshed here.
                let probe = probe::gather(&catalog, &prefs, &exe, false, &|_| true, &mut |n| {
                    notes.push(n.to_string());
                    let text = json!({ "state": "checking", "notes": notes }).to_string();
                    let _ = qt.queue(move |b| b.set_check_json(QString::from(&text)));
                });
                let u = Upgrade::new(probe, prefs);
                let text = u.check().to_string();
                let _ = qt.queue(move |mut b| {
                    b.as_mut().rust_mut().get_mut().upgrade = Some(u);
                    b.as_mut().rust_mut().get_mut().busy = false;
                    b.set_check_json(QString::from(&text));
                });
            })
            .expect("spawning the check thread");
    }

    fn toggle(mut self: Pin<&mut Self>, id: &QString) {
        let changed =
            self.as_mut().rust_mut().get_mut().upgrade.as_mut().map(|u| (u.toggle(&id.to_string()), u.check()));
        if let Some((true, check)) = changed {
            self.as_mut().set_check_json(qs(&check));
        }
    }

    fn start_run(mut self: Pin<&mut Self>) {
        if self.rust().busy {
            return;
        }
        let Some(u) = self.rust().upgrade.as_ref() else { return };
        let (plans, answers, aur_terminal) = (u.plans(), u.answers(), u.aur_in_terminal());
        let new_features = u.probe().after.setup.whats_new.len();
        let o = u.run_options();
        let before = u.probe().after.clone();
        let dry_run = self.rust().dry_run || self.screenshots();
        let sandbox = self.sandbox_dir().map(PathBuf::from);
        let rehearsal = dry_run || sandbox.is_some();
        self.as_mut().rust_mut().get_mut().busy = true;
        self.as_mut().set_run_json(qs(&json!(RunState::starting(&plans, rehearsal))));
        let qt = self.qt_thread();
        std::thread::Builder::new()
            .name("upgrade".into())
            .spawn(move || {
                let mut after = |updated: &[String], boot: Option<Vec<String>>| {
                    // A rehearsal changed nothing: the first look still holds.
                    let mut now = if rehearsal { before.clone() } else { probe::after() };
                    now.boot = boot.or(now.boot);
                    after_plans(&before, &now, updated, &o, false)
                };
                let how = How { dry_run, sandbox, aur_terminal, new_features };
                let publish = |s: &RunState| {
                    let text = serde_json::to_string(s).unwrap_or_default();
                    let _ = qt.queue(move |b| b.set_run_json(QString::from(&text)));
                };
                let last = crate::upgrade::run::run(plans, answers, &mut after, how, publish);
                let text = serde_json::to_string(&last).unwrap_or_default();
                let _ = qt.queue(move |mut b| {
                    b.as_mut().rust_mut().get_mut().busy = false;
                    b.set_run_json(QString::from(&text));
                });
            })
            .expect("spawning the upgrade thread");
    }

    fn open_terminal(mut self: Pin<&mut Self>, only_aur: bool) {
        let sandbox = self.sandbox_dir();
        let note = match crate::upgrade::run::in_terminal(only_aur, self.rust().dry_run, sandbox.as_deref()) {
            Ok(()) => String::new(),
            Err(e) => format!("Couldn't open a terminal: {e:#}"),
        };
        self.as_mut().set_notice(QString::from(&note));
    }

    fn reboot(mut self: Pin<&mut Self>) {
        if self.sandbox_dir().is_none()
            && !self.rust().dry_run
            && !self.screenshots()
            && let Some(note) = crate::apply::reboot()
        {
            self.as_mut().set_notice(QString::from(&note));
        }
    }

    fn open_bindle(mut self: Pin<&mut Self>) {
        let opened = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg("--whats-new").spawn());
        let note = match opened {
            Ok(_) => String::new(),
            Err(e) => format!("Couldn't open Bindle: {e}"),
        };
        self.as_mut().set_notice(QString::from(&note));
    }

    fn reopen(mut self: Pin<&mut Self>) {
        let started = std::env::current_exe().and_then(|exe| {
            std::process::Command::new(exe).arg("--upgrade").env(bindle_upgrade::selfupdate::UPDATED_ENV, "1").spawn()
        });
        match started {
            Ok(_) => std::process::exit(0),
            Err(e) => self.as_mut().set_notice(QString::from(&format!("Couldn't open the new window: {e}"))),
        }
    }

    fn load_sample(mut self: Pin<&mut Self>, name: &QString) {
        if !self.screenshots() {
            return;
        }
        let probe = if name.to_string() == "held" { sample::held() } else { sample::probe() };
        let u = Upgrade::new(probe, prefs::Prefs::default());
        self.as_mut().set_check_json(qs(&u.check()));
        self.as_mut().set_run_json(qs(&json!({ "state": "idle" })));
        self.as_mut().rust_mut().get_mut().upgrade = Some(u);
    }

    fn grab_window(&self, path: &QString) -> bool {
        crate::app::ffi::bindle_grab_window(&path.to_string())
    }

    fn glyph(&self, name: &QString, color: &QString) -> QString {
        QString::from(&crate::glyph::url(&name.to_string(), &color.to_string()))
    }
}
