//! The QML-facing `Backend` object: a thin bridge over [`crate::model::Wizard`].
//! Screens read JSON properties; QML calls invokables, which update the
//! wizard and refresh the properties that changed.

use std::path::{Path, PathBuf};
use std::pin::Pin;

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
        /// "What's new" after a Bindle upgrade (`Wizard::whats_new_json`).
        #[qproperty(QString, whats_new_json)]
        type Backend = super::BackendRust;

        #[qinvokable]
        fn choose_profile(self: Pin<&mut Backend>, id: &QString);
        #[qinvokable]
        fn toggle(self: Pin<&mut Backend>, id: &QString);
        /// Tick or untick an add-on profile (Gaming, Developer).
        #[qinvokable]
        fn toggle_addon(self: Pin<&mut Backend>, id: &QString);
        /// "Select all" / "Select none" for a group: `ids` is a JSON array.
        #[qinvokable]
        fn set_group(self: Pin<&mut Backend>, ids: &QString, on: bool, group: &QString);
        /// Open `bindle undo` in a terminal (it asks for the password there).
        #[qinvokable]
        fn undo_in_terminal(self: Pin<&mut Backend>);
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
        /// Tick or untick a What's new item.
        #[qinvokable]
        fn toggle_whats_new(self: Pin<&mut Backend>, id: &QString);
        /// Open What's new (Review and Apply follow its picks) or leave it
        /// for the wizard.
        #[qinvokable]
        fn set_whats_new(self: Pin<&mut Backend>, on: bool);
        /// Tick or untick one change of a What's new item.
        #[qinvokable]
        fn toggle_whats_new_change(self: Pin<&mut Backend>, id: &QString, key: &QString);
        /// Hide a What's new item for good (until it changes again).
        #[qinvokable]
        fn hide_whats_new(self: Pin<&mut Backend>, id: &QString);
        /// Done with this version: What's new opens by itself again for a newer Bindle.
        #[qinvokable]
        fn skip_whats_new(self: Pin<&mut Backend>);
        /// Screenshots: a made-up What's new.
        #[qinvokable]
        fn sample_whats_new(self: Pin<&mut Backend>);
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
    whats_new_json: QString,
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
        // What's new: asked for (`--whats-new`), or the first run of a newer Bindle.
        let asked = crate::flag("--whats-new");
        let shots = crate::arg("--screenshots").is_some();
        if !shots && (asked || real) {
            let root = self.root();
            self.as_mut().wizard_mut().load_whats_new(&root, !asked);
            self.as_mut().wizard_mut().set_whats_new_active(true);
        }
        self.as_mut().refresh_whats_new();

        let path = theme::tokens_path();
        self.as_mut().set_theme_json(json(&theme::load(path.as_ref())));
        let qt = self.qt_thread();
        theme::watch(path, move |tokens| qt.queue(move |b| b.set_theme_json(QString::from(&tokens))).is_ok());
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
        let (profile, state, profiles) = {
            let w = self.wizard();
            (QString::from(w.profile()), json(&w.state()), json(&w.profiles()))
        };
        self.as_mut().set_profile(profile);
        self.as_mut().set_state_json(state);
        // The add-on ticks and the "you've edited" flag follow the selection.
        self.as_mut().set_profiles_json(profiles);
    }

    fn choose_profile(mut self: Pin<&mut Self>, id: &QString) {
        self.as_mut().wizard_mut().set_profile(&id.to_string());
        self.as_mut().set_feedback_json(QString::default());
        self.refresh_selection();
    }

    fn toggle(mut self: Pin<&mut Self>, id: &QString) {
        let change = self.as_mut().wizard_mut().toggle(&id.to_string());
        self.publish(change);
    }

    fn toggle_addon(mut self: Pin<&mut Self>, id: &QString) {
        self.as_mut().wizard_mut().toggle_addon(&id.to_string());
        self.refresh_selection();
    }

    fn set_group(mut self: Pin<&mut Self>, ids: &QString, on: bool, group: &QString) {
        let ids: Vec<String> = serde_json::from_str(&ids.to_string()).unwrap_or_default();
        let change = self.as_mut().wizard_mut().set_all(&ids, on, &group.to_string());
        self.publish(change);
    }

    fn undo_in_terminal(mut self: Pin<&mut Self>) {
        let sandbox = self.rust().sandbox.to_string();
        let note = match crate::apply::undo_in_terminal((!sandbox.is_empty()).then_some(sandbox.as_str())) {
            Ok(()) => String::new(),
            Err(e) => format!("Couldn't open a terminal: {e:#}"),
        };
        self.as_mut().set_notice(QString::from(&note));
    }

    /// Show what a click changed, then refresh what depends on the selection.
    fn publish(mut self: Pin<&mut Self>, change: Option<crate::model::Change>) {
        let Some(change) = change else {
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
        if self.wizard().whats_new_active() && self.real_run() {
            // Done with this version: it opens by itself again for a newer Bindle.
            let _ = bindle_run::whatsnew_state::skip_this_version();
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

    fn refresh_whats_new(mut self: Pin<&mut Self>) {
        let j = json(&self.wizard().whats_new_json());
        self.as_mut().set_whats_new_json(j);
    }

    fn toggle_whats_new(mut self: Pin<&mut Self>, id: &QString) {
        self.as_mut().wizard_mut().toggle_whats_new(&id.to_string());
        self.refresh_whats_new();
    }

    /// "Not now" leaves it (nothing stored: it asks again next time).
    fn set_whats_new(mut self: Pin<&mut Self>, on: bool) {
        self.as_mut().wizard_mut().set_whats_new_active(on);
        self.refresh_whats_new();
    }

    fn real_run(&self) -> bool {
        self.rust().sandbox.is_empty() && self.rust().screenshot_dir.is_empty()
    }

    fn toggle_whats_new_change(mut self: Pin<&mut Self>, id: &QString, key: &QString) {
        self.as_mut().wizard_mut().toggle_whats_new_change(&id.to_string(), &key.to_string());
        self.refresh_whats_new();
    }

    fn hide_whats_new(mut self: Pin<&mut Self>, id: &QString) {
        let persist = self.real_run();
        let note = match self.as_mut().wizard_mut().hide_whats_new(&id.to_string(), persist) {
            Ok(()) => String::new(),
            Err(e) => format!("Couldn't remember that: {e:#}"),
        };
        self.as_mut().set_notice(QString::from(&note));
        self.refresh_whats_new();
    }

    fn skip_whats_new(self: Pin<&mut Self>) {
        if self.real_run() {
            let _ = bindle_run::whatsnew_state::skip_this_version();
        }
        self.set_whats_new(false);
    }

    fn sample_whats_new(mut self: Pin<&mut Self>) {
        self.as_mut().wizard_mut().sample_whats_new();
        self.as_mut().wizard_mut().set_whats_new_active(true);
        self.refresh_whats_new();
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
