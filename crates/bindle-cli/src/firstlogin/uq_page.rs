//! uQ Settings' Bindle page (`uq/pages/bindle/`): every file of it, embedded, written to
//! `/usr/share/uq/settings/pages/bindle/` by `first-login install`.

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

/// The installed path of every file.
pub fn paths() -> Vec<String> {
    FILES.iter().map(|(name, _)| format!("{DIR}/{name}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_of_the_page_folder_is_embedded() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../uq/pages/bindle");
        let mut on_disk: Vec<String> =
            std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        on_disk.sort();
        let mut embedded: Vec<String> = FILES.iter().map(|(n, _)| n.to_string()).collect();
        embedded.sort();
        assert_eq!(on_disk, embedded, "a file of uq/pages/bindle is missing from FILES (or the other way round)");
    }
}
