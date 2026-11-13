//! The QML-facing `Backend` object: a thin bridge over [`crate::model::Wizard`].
//! Screens read JSON properties; QML calls invokables, which update the
//! wizard and refresh the properties that changed.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use bindle_core::{Catalog, Facts};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use serde_json::Value;

use crate::apply::ApplyState;
use crate::model::Wizard;
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
        /// Distro, CPU, RAM, GPUs and form factor, for the title bar.
        #[qproperty(QString, header)]
        #[qproperty(QString, welcome_json)]
        #[qproperty(QString, profiles_json)]
        #[qproperty(QString, profile)]
        /// Categories, groups and modules: fixed for the run (`Wizard::catalog`).
        #[qproperty(QString, catalog_json)]
        /// What is ticked right now (`Wizard::state`).
        #[qproperty(QString, state_json)]
        /// The last click's effect: `{"text", "tone", "seq"}`.
        #[qproperty(QString, feedback_json)]
        #[qproperty(QString, review_json)]
        /// uQ's theme tokens, or `{"present": false}` off uQ.
        #[qproperty(QString, theme_json)]
        /// A one-line message for the footer (the terminal couldn't open).
        #[qproperty(QString, notice)]
        /// `--screenshots <dir>`: QML walks the screens and saves each here.
        #[qproperty(QString, screenshot_dir)]
        /// `--sandbox <dir>`: apply records commands under this directory.
        #[qproperty(QString, sandbox)]
        /// The Apply screen: `crate::apply::ApplyState`.
        #[qproperty(QString, apply_json)]
        type Backend = super::BackendRust;

        #[qinvokable]
        fn choose_profile(self: Pin<&mut Backend>, id: &QString);
        #[qinvokable]
        fn toggle(self: Pin<&mut Backend>, id: &QString);
        /// Resolve the plan for the Review screen.
        #[qinvokable]
        fn refresh_review(self: Pin<&mut Backend>);
        /// Run the reviewed plan; progress arrives in `apply_json`.
        #[qinvokable]
        fn start_apply(self: Pin<&mut Backend>);
        /// No polkit agent: finish in a terminal instead.
        #[qinvokable]
        fn continue_in_terminal(self: Pin<&mut Backend>);
        #[qinvokable]
        fn reboot(self: &Backend);
        /// Save the window as a PNG (screenshot tests).
        #[qinvokable]
        fn grab_window(self: &Backend, path: &QString) -> bool;
        /// A theme icon drawn in one colour (`#rrggbb`), as a `data:` URL;
        /// empty when no installed theme has it.
        #[qinvokable]
        fn glyph(self: &Backend, name: &QString, color: &QString) -> QString;
    }

    impl cxx_qt::Threading for Backend {}
    impl cxx_qt::Initialize for Backend {}
}

use qobject::Backend;

#[derive(Default)]
pub struct BackendRust {
    header: QString,
    welcome_json: QString,
    profiles_json: QString,
    profile: QString,
    catalog_json: QString,
    state_json: QString,
    feedback_json: QString,
    clicks: u32,
    review_json: QString,
    theme_json: QString,
    notice: QString,
    screenshot_dir: QString,
    sandbox: QString,
    apply_json: QString,
    wizard: Option<Wizard>,
    applying: bool,
}

fn json(v: &Value) -> QString {
    QString::from(&v.to_string())
}

impl cxx_qt::Initialize for Backend {
    fn initialize(mut self: Pin<&mut Self>) {
        let facts = Facts::detect();
        let catalog = Catalog::arch().expect("the built-in catalog is validated by the tests").with_drives(&facts);
        let mut wizard = Wizard::new(catalog, facts);
        // Like the terminal wizard: carry on from the last selection (not in
        // screenshot or sandbox runs, which must not depend on this machine).
        let real = crate::arg("--screenshots").is_none() && crate::arg("--sandbox").is_none();
        if real
            && let Some(sel) = bindle_run::selection::default_path().and_then(|p| bindle_run::selection::load(&p).ok())
        {
            wizard.seed(&sel);
        }
        self.as_mut().set_header(QString::from(&wizard.header()));
        self.as_mut().set_welcome_json(json(&wizard.welcome()));
        self.as_mut().set_profiles_json(json(&wizard.profiles()));
        self.as_mut().set_catalog_json(json(&wizard.catalog()));
        self.as_mut().rust_mut().wizard = Some(wizard);
        self.as_mut().refresh_selection();
        if let Some(dir) = crate::arg("--screenshots") {
            self.as_mut().set_screenshot_dir(QString::from(&dir));
        }
        if let Some(dir) = crate::arg("--sandbox") {
            self.as_mut().set_sandbox(QString::from(&dir));
        }
        self.as_mut().set_apply_json(QString::from(r#"{"state":"idle"}"#));

        let path = theme::tokens_path();
        self.as_mut().set_theme_json(json(&theme::load(path.as_ref())));
        // uqd rewrites tokens.json when the theme or variant changes; follow it.
        let qt = self.qt_thread();
        std::thread::Builder::new()
            .name("uq-theme".into())
            .spawn(move || {
                let mut seen = path.as_ref().and_then(theme::modified);
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                    let now = path.as_ref().and_then(theme::modified);
                    if now != seen {
                        seen = now;
                        let tokens = json(&theme::load(path.as_ref())).to_string();
                        if qt.queue(move |b| b.set_theme_json(QString::from(&tokens))).is_err() {
                            return; // the window is gone
                        }
                    }
                }
            })
            .expect("spawning the theme watcher");
    }
}

impl Backend {
    fn wizard(&self) -> &Wizard {
        self.rust().wizard.as_ref().expect("set in initialize")
    }

    fn wizard_mut(self: Pin<&mut Self>) -> &mut Wizard {
        self.rust_mut().get_mut().wizard.as_mut().expect("set in initialize")
    }

    /// The properties that depend on the selection.
    fn refresh_selection(mut self: Pin<&mut Self>) {
        let (profile, state) = {
            let w = self.wizard();
            (QString::from(w.profile()), json(&w.state()))
        };
        self.as_mut().set_profile(profile);
        self.as_mut().set_state_json(state);
    }

    fn choose_profile(mut self: Pin<&mut Self>, id: &QString) {
        self.as_mut().wizard_mut().set_profile(&id.to_string());
        self.as_mut().set_feedback_json(QString::default());
        self.refresh_selection();
    }

    fn toggle(mut self: Pin<&mut Self>, id: &QString) {
        let Some(change) = self.as_mut().wizard_mut().toggle(&id.to_string()) else {
            return;
        };
        // `seq` makes two identical lines in a row still count as a change.
        self.as_mut().rust_mut().get_mut().clicks += 1;
        let seq = self.rust().clicks;
        let feedback = serde_json::json!({ "text": change.text, "tone": change.tone, "seq": seq });
        self.as_mut().set_feedback_json(json(&feedback));
        self.refresh_selection();
    }

    fn root(&self) -> PathBuf {
        let sandbox = self.rust().sandbox.to_string();
        if sandbox.is_empty() { PathBuf::from("/") } else { PathBuf::from(sandbox) }
    }

    fn refresh_review(mut self: Pin<&mut Self>) {
        let review = json(&self.wizard().review(&self.root()));
        self.as_mut().set_review_json(review);
    }

    fn start_apply(mut self: Pin<&mut Self>) {
        if self.rust().applying {
            return;
        }
        let root = self.root();
        let plan = match self.wizard().plan(&root) {
            Ok(p) => p,
            Err(e) => {
                let state = ApplyState { state: "error".into(), error: format!("{e:#}"), ..Default::default() };
                self.as_mut().set_apply_json(json(&serde_json::json!(state)));
                return;
            }
        };
        let facts = self.wizard().facts().clone();
        let sel = self.wizard().selection();
        let sandbox = (root != Path::new("/")).then_some(root);
        self.as_mut().rust_mut().get_mut().applying = true;
        self.as_mut().set_apply_json(json(&serde_json::json!(ApplyState::starting(&plan))));
        let qt = self.qt_thread();
        std::thread::Builder::new()
            .name("apply".into())
            .spawn(move || {
                let publish = |s: &ApplyState| {
                    let text = serde_json::to_string(s).unwrap_or_default();
                    let _ = qt.queue(move |b| b.set_apply_json(QString::from(&text)));
                };
                let last = crate::apply::run(&plan, &facts, &sel, sandbox, publish);
                let text = serde_json::to_string(&last).unwrap_or_default();
                let _ = qt.queue(move |mut b| {
                    b.as_mut().rust_mut().get_mut().applying = false;
                    b.set_apply_json(QString::from(&text));
                });
            })
            .expect("spawning the apply thread");
    }

    fn continue_in_terminal(mut self: Pin<&mut Self>) {
        let note = match crate::apply::continue_in_terminal(&self.wizard().selection()) {
            Ok(()) => String::new(),
            Err(e) => format!("Couldn't open a terminal: {e:#}"),
        };
        self.as_mut().set_notice(QString::from(&note));
    }

    fn reboot(&self) {
        if self.rust().sandbox.is_empty() {
            let _ = std::process::Command::new("systemctl").arg("reboot").spawn();
        }
    }

    fn grab_window(&self, path: &QString) -> bool {
        crate::app::ffi::bindle_grab_window(&path.to_string())
    }

    fn glyph(&self, name: &QString, color: &QString) -> QString {
        QString::from(&crate::glyph::url(&name.to_string(), &color.to_string()))
    }
}
