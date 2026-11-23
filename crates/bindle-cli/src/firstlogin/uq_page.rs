//! uQ Settings' Bindle page (`uq/pages/bindle/`) and bindleUP's bar icon
//! (`uq/widgets/bindleup/`): every file of them, embedded, written to
//! `/usr/share/uq/settings/pages/bindle/` and `/usr/share/uq/shell/widgets/bindleup/` by
//! `first-login install`.

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

/// Every file with its installed path: the page's, then the widget's.
pub fn installed() -> Vec<(String, &'static str)> {
    let page = FILES.iter().map(|(name, text)| (format!("{DIR}/{name}"), *text));
    let widget = WIDGET_FILES.iter().map(|(name, text)| (format!("{WIDGET_DIR}/{name}"), *text));
    page.chain(widget).collect()
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
        assert_eq!(paths().len(), FILES.len() + WIDGET_FILES.len());
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
    }
}
