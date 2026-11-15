//! The QML-facing `RecoveryBackend`: a thin bridge over
//! [`crate::recovery`] for the Recovery window (`bindle-gui --recovery`,
//! `RecoveryMain.qml`). Commands run on worker threads; pages read JSON.

use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use serde_json::json;

use crate::recovery::{self, RunState};
use crate::theme;

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
        #[qproperty(QString, theme_json)]
        /// `recovery::status`: what this computer allows.
        #[qproperty(QString, status_json)]
        /// The snapshots (`bindle rollback --list --json`), or
        /// `{"state": "loading"}` / `{"error": …}`.
        #[qproperty(QString, snapshots_json)]
        /// `recovery::RunState`.
        #[qproperty(QString, run_json)]
        #[qproperty(QString, notice)]
        #[qproperty(QString, screenshot_dir)]
        #[qproperty(QString, sandbox)]
        #[qproperty(bool, dry_run)]
        type RecoveryBackend = super::RecoveryRust;

        #[qinvokable]
        fn refresh(self: Pin<&mut RecoveryBackend>);
        /// List the snapshots (asks for the password: they are root's).
        #[qinvokable]
        fn load_snapshots(self: Pin<&mut RecoveryBackend>);
        /// rollback (with `number`), rebase, restore, keep-on, keep-off, cancel.
        #[qinvokable]
        fn start(self: Pin<&mut RecoveryBackend>, kind: &QString, number: i32);
        #[qinvokable]
        fn reboot(self: Pin<&mut RecoveryBackend>);
        #[qinvokable]
        fn grab_window(self: &RecoveryBackend, path: &QString) -> bool;
        #[qinvokable]
        fn glyph(self: &RecoveryBackend, name: &QString, color: &QString) -> QString;
    }

    impl cxx_qt::Threading for RecoveryBackend {}
    impl cxx_qt::Initialize for RecoveryBackend {}
}

use qobject::RecoveryBackend;

#[derive(Default)]
pub struct RecoveryRust {
    theme_json: QString,
    status_json: QString,
    snapshots_json: QString,
    run_json: QString,
    notice: QString,
    screenshot_dir: QString,
    sandbox: QString,
    dry_run: bool,
    busy: bool,
}

fn qs(v: &serde_json::Value) -> QString {
    QString::from(&v.to_string())
}

impl cxx_qt::Initialize for RecoveryBackend {
    fn initialize(mut self: Pin<&mut Self>) {
        if let Some(dir) = crate::arg("--screenshots") {
            self.as_mut().set_screenshot_dir(QString::from(&dir));
        }
        if let Some(dir) = crate::arg("--sandbox") {
            self.as_mut().set_sandbox(QString::from(&dir));
        }
        self.as_mut().set_dry_run(crate::flag("--dry-run") || crate::flag("-n"));
        self.as_mut().set_snapshots_json(qs(&json!({ "state": "none" })));
        self.as_mut().set_run_json(qs(&json!(RunState { state: "idle".into(), ..Default::default() })));
        self.as_mut().refresh();
        let path = theme::tokens_path();
        self.as_mut().set_theme_json(qs(&theme::load(path.as_ref())));
        let qt = self.qt_thread();
        theme::watch(path, move |tokens| qt.queue(move |b| b.set_theme_json(QString::from(&tokens))).is_ok());
    }
}

impl RecoveryBackend {
    fn screenshots(&self) -> bool {
        !self.rust().screenshot_dir.is_empty()
    }

    fn sandbox_dir(&self) -> Option<String> {
        let s = self.rust().sandbox.to_string();
        (!s.is_empty()).then_some(s)
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let status = if self.screenshots() { recovery::sample_status() } else { recovery::status() };
        self.as_mut().set_status_json(qs(&status));
    }

    fn load_snapshots(mut self: Pin<&mut Self>) {
        if self.screenshots() {
            self.as_mut().set_snapshots_json(qs(&recovery::sample_snapshots()));
            return;
        }
        if self.rust().busy {
            return;
        }
        self.as_mut().rust_mut().get_mut().busy = true;
        self.as_mut().set_snapshots_json(qs(&json!({ "state": "loading" })));
        let args = recovery::argv("snapshots", 0, false, None).expect("a known action");
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let mut out = vec![];
            let ok = recovery::run(&recovery::bindle(), &args, &mut |l| out.push(l));
            let json_line = out.iter().rev().find(|l| l.starts_with('['));
            let value = match (ok, json_line.and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok())) {
                (true, Some(list)) => list,
                _ => json!({ "error": out.last().cloned().unwrap_or_else(|| "couldn't list the snapshots".into()) }),
            };
            let _ = qt.queue(move |mut b| {
                b.as_mut().rust_mut().get_mut().busy = false;
                b.set_snapshots_json(qs(&value));
            });
        });
    }

    fn start(mut self: Pin<&mut Self>, kind: &QString, number: i32) {
        let kind = kind.to_string();
        if self.rust().busy {
            return;
        }
        let restart = matches!(kind.as_str(), "rollback" | "restore");
        if self.screenshots() {
            let log = recovery::SAMPLE_RUN.iter().map(|l| l.to_string()).collect();
            let s = RunState { state: "done".into(), kind, log, restart };
            self.as_mut().set_run_json(qs(&json!(s)));
            return;
        }
        let dry = self.rust().dry_run;
        let Some(args) = recovery::argv(&kind, number.max(0) as u32, dry, self.sandbox_dir().as_deref()) else {
            return;
        };
        self.as_mut().rust_mut().get_mut().busy = true;
        let mut state = RunState { state: "running".into(), kind, log: vec![], restart };
        self.as_mut().set_run_json(qs(&json!(state)));
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let ok = recovery::run(&recovery::bindle(), &args, &mut |l| {
                state.log.push(l);
                let text = json!(state).to_string();
                let _ = qt.queue(move |b| b.set_run_json(QString::from(&text)));
            });
            state.state = if ok { "done" } else { "failed" }.into();
            state.restart &= ok && !dry;
            let text = json!(state).to_string();
            let _ = qt.queue(move |mut b| {
                b.as_mut().rust_mut().get_mut().busy = false;
                b.as_mut().set_run_json(QString::from(&text));
                b.as_mut().refresh();
            });
        });
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

    fn grab_window(&self, path: &QString) -> bool {
        crate::app::ffi::bindle_grab_window(&path.to_string())
    }

    fn glyph(&self, name: &QString, color: &QString) -> QString {
        QString::from(&crate::glyph::url(&name.to_string(), &color.to_string()))
    }
}
