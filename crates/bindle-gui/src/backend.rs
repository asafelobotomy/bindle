//! The QML-facing `Backend` object: a thin bridge over [`crate::model::Wizard`].
//! Screens read JSON properties; QML calls invokables, which update the
//! wizard and refresh the properties that changed.

use std::pin::Pin;
use std::time::Duration;

use bindle_core::{Catalog, Facts};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use serde_json::Value;

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
        #[qproperty(QString, categories_json)]
        #[qproperty(QString, review_json)]
        /// uQ's theme tokens, or `{"present": false}` off uQ.
        #[qproperty(QString, theme_json)]
        /// A one-line message after a toggle (a rival module was dropped).
        #[qproperty(QString, notice)]
        /// `--screenshots <dir>`: QML walks the screens and saves each here.
        #[qproperty(QString, screenshot_dir)]
        type Backend = super::BackendRust;

        #[qinvokable]
        fn choose_profile(self: Pin<&mut Backend>, id: &QString);
        #[qinvokable]
        fn toggle(self: Pin<&mut Backend>, id: &QString);
        /// Resolve the plan for the Review screen.
        #[qinvokable]
        fn refresh_review(self: Pin<&mut Backend>);
        /// Save the window as a PNG (screenshot tests).
        #[qinvokable]
        fn grab_window(self: &Backend, path: &QString) -> bool;
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
    categories_json: QString,
    review_json: QString,
    theme_json: QString,
    notice: QString,
    screenshot_dir: QString,
    wizard: Option<Wizard>,
}

fn json(v: &Value) -> QString {
    QString::from(&v.to_string())
}

impl cxx_qt::Initialize for Backend {
    fn initialize(mut self: Pin<&mut Self>) {
        let catalog = Catalog::arch().expect("the built-in catalog is validated by the tests");
        let wizard = Wizard::new(catalog, Facts::detect());
        self.as_mut().set_header(QString::from(&wizard.header()));
        self.as_mut().set_welcome_json(json(&wizard.welcome()));
        self.as_mut().set_profiles_json(json(&wizard.profiles()));
        self.as_mut().rust_mut().wizard = Some(wizard);
        self.as_mut().refresh_selection();
        if let Some(dir) = crate::screenshot_dir() {
            self.as_mut().set_screenshot_dir(QString::from(&dir));
        }

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
        let (profile, categories) = {
            let w = self.wizard();
            (QString::from(w.profile()), json(&w.categories()))
        };
        self.as_mut().set_profile(profile);
        self.as_mut().set_categories_json(categories);
    }

    fn choose_profile(mut self: Pin<&mut Self>, id: &QString) {
        self.as_mut().wizard_mut().set_profile(&id.to_string());
        self.as_mut().set_notice(QString::default());
        self.refresh_selection();
    }

    fn toggle(mut self: Pin<&mut Self>, id: &QString) {
        let notice = self.as_mut().wizard_mut().toggle(&id.to_string()).unwrap_or_default();
        self.as_mut().set_notice(QString::from(&notice));
        self.refresh_selection();
    }

    fn refresh_review(mut self: Pin<&mut Self>) {
        let review = json(&self.wizard().review());
        self.as_mut().set_review_json(review);
    }

    fn grab_window(&self, path: &QString) -> bool {
        crate::app::ffi::bindle_grab_window(&path.to_string())
    }
}
