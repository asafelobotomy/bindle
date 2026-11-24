//! uQ Settings' Bindle page (`uq/pages/bindle/`) and bindleUP's bar icon
//! (`uq/widgets/bindleup/`): every file of them, embedded, written to
//! `/usr/share/uq/settings/pages/bindle/` and `/usr/share/uq/shell/widgets/bindleup/` by
//! `first-login install` (and again by `first-login refresh` after bindleup replaced Bindle),
//! with Bindle's problem codes (`uq/diag/bindle.toml`) for `uqctl explain` and Settings › Problems.

pub const DIR: &str = "/usr/share/uq/settings/pages/bindle";

/// File name and contents. `page.toml` first: uQ reads it to find the rest.
pub const FILES: &[(&str, &str)] = &[
    ("page.toml", include_str!("../../../../uq/pages/bindle/page.toml")),
    ("Page.qml", include_str!("../../../../uq/pages/bindle/Page.qml")),
    ("logic.js", include_str!("../../../../uq/pages/bindle/logic.js")),
    ("Outcome.qml", include_str!("../../../../uq/pages/bindle/Outcome.qml")),
    ("ProgressView.qml", include_str!("../../../../uq/pages/bindle/ProgressView.qml")),
    ("SetupTab.qml", include_str!("../../../../uq/pages/bindle/SetupTab.qml")),
    ("UpdatesTab.qml", include_str!("../../../../uq/pages/bindle/UpdatesTab.qml")),
    ("WhatsNewTab.qml", include_str!("../../../../uq/pages/bindle/WhatsNewTab.qml")),
    ("HistoryTab.qml", include_str!("../../../../uq/pages/bindle/HistoryTab.qml")),
    ("PreferencesTab.qml", include_str!("../../../../uq/pages/bindle/PreferencesTab.qml")),
];

pub const WIDGET_DIR: &str = "/usr/share/uq/shell/widgets/bindleup";

/// bindleUP's bar icon. `widget.toml` first, as for the page.
pub const WIDGET_FILES: &[(&str, &str)] = &[
    ("widget.toml", include_str!("../../../../uq/widgets/bindleup/widget.toml")),
    ("Widget.qml", include_str!("../../../../uq/widgets/bindleup/Widget.qml")),
];

/// Bindle's problem codes, where uQ reads them.
pub const DIAG: &str = "/usr/share/uq/diag/bindle.toml";
const DIAG_FILE: &str = include_str!("../../../../uq/diag/bindle.toml");

/// The AUR package's page shows only while `/usr/bin/bindle` exists (`[requires] bins`, which uQ
/// looks up in /usr/bin alone). Installed here, Bindle is `/usr/local/bin/bindle`, so the same
/// table would hide the page for good; `first-login remove --purge` takes the page away instead.
const REQUIRES: &str = "[requires]\nbins = [\"bindle\"]\n";

/// Every file with its installed path: the page's, the widget's, then the problem codes.
pub fn installed() -> Vec<(String, String)> {
    let page = FILES.iter().map(|(name, text)| {
        let text = if *name == "page.toml" { text.replacen(REQUIRES, "", 1) } else { text.to_string() };
        (format!("{DIR}/{name}"), text)
    });
    let widget = WIDGET_FILES.iter().map(|(name, text)| (format!("{WIDGET_DIR}/{name}"), text.to_string()));
    page.chain(widget).chain([(DIAG.to_string(), DIAG_FILE.to_string())]).collect()
}

/// The installed path of every file.
pub fn paths() -> Vec<String> {
    installed().into_iter().map(|(path, _)| path).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same_files(folder: &str, files: &[(&str, &str)]) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(folder);
        let mut on_disk: Vec<String> =
            std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        on_disk.sort();
        let mut embedded: Vec<String> = files.iter().map(|(n, _)| n.to_string()).collect();
        embedded.sort();
        assert_eq!(on_disk, embedded, "a file of {folder} isn't embedded (or the other way round)");
    }

    #[test]
    fn every_file_of_the_page_folder_is_embedded() {
        same_files("uq/pages/bindle", FILES);
    }

    #[test]
    fn every_file_of_the_widget_folder_is_embedded() {
        same_files("uq/widgets/bindleup", WIDGET_FILES);
        assert_eq!(paths().len(), FILES.len() + WIDGET_FILES.len() + 1);
    }

    /// uQ hides a page whose `[requires] bins` aren't in /usr/bin; installed here Bindle is in
    /// /usr/local/bin, so that table must go (and only it).
    #[test]
    fn the_installed_page_does_not_require_usr_bin_bindle() {
        assert!(FILES[0].1.contains(REQUIRES), "page.toml's [requires] changed: update REQUIRES");
        let all = installed();
        let page = &all.iter().find(|(p, _)| p == &format!("{DIR}/page.toml")).unwrap().1;
        assert!(!page.contains("[requires]"));
        let parsed: toml::Value = toml::from_str(page).unwrap();
        assert!(parsed.get("requires").is_none());
        assert_eq!(parsed["command"].as_array().unwrap().len(), {
            let original: toml::Value = toml::from_str(FILES[0].1).unwrap();
            original["command"].as_array().unwrap().len()
        });
        assert!(all.iter().any(|(p, t)| p == DIAG && t.contains("BINDLE-001")));
    }
}

#[cfg(test)]
mod consts_tests {
    use crate::uq_consts::install;

    /// The folders are uQ's (uq-consts), where uQ reads integrations from.
    #[test]
    fn the_page_and_widget_go_where_uq_reads_them() {
        assert_eq!(super::DIR, format!("{}/bindle", install::PAGES_DIR));
        assert_eq!(super::WIDGET_DIR, format!("{}/bindleup", install::WIDGETS_DIR));
        assert_eq!(crate::firstlogin::UQ_PAGE, format!("{}/bindle/page.toml", install::PAGES_DIR));
        assert_eq!(super::DIAG, format!("{}/bindle.toml", install::DIAG_DIR));
    }
}
