use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    let builder = CxxQtBuilder::new_qml_module(QmlModule::new("org.bindle.wizard").qml_files([
        QmlFile::from("qml/Main.qml"),
        QmlFile::from("qml/Theme.qml").singleton(true),
        QmlFile::from("qml/Panel.qml"),
        QmlFile::from("qml/Pill.qml"),
        QmlFile::from("qml/ActionButton.qml"),
        QmlFile::from("qml/StepBar.qml"),
        QmlFile::from("qml/WelcomePage.qml"),
        QmlFile::from("qml/ProfilePage.qml"),
        QmlFile::from("qml/ChoosePage.qml"),
        QmlFile::from("qml/ModuleDetails.qml"),
        QmlFile::from("qml/ModuleSection.qml"),
        QmlFile::from("qml/ModuleCard.qml"),
        QmlFile::from("qml/CheckMark.qml"),
        QmlFile::from("qml/Chip.qml"),
        QmlFile::from("qml/SummaryPage.qml"),
        QmlFile::from("qml/ReviewPage.qml"),
        QmlFile::from("qml/ApplyPage.qml"),
        QmlFile::from("qml/ProfileCard.qml"),
        QmlFile::from("qml/ReviewList.qml"),
        QmlFile::from("qml/Notice.qml"),
        QmlFile::from("qml/Disclosure.qml"),
        QmlFile::from("qml/Glyph.qml"),
        QmlFile::from("qml/WhatsNewPage.qml"),
        QmlFile::from("qml/UpgradeMain.qml"),
        QmlFile::from("qml/UpgradeCheckPage.qml"),
        QmlFile::from("qml/UpgradeRunPage.qml"),
    ]))
    .qt_module("Gui")
    .qt_module("Quick")
    .qt_module("QuickControls2")
    .files(["src/backend.rs", "src/upgrade_backend.rs", "src/app.rs"])
    .cpp_file("cpp/app.cpp");
    // SAFETY: only adds a warning flag; GCC 16 warns about Qt's own headers.
    let builder = unsafe {
        builder.cc_builder(|cc| {
            cc.flag_if_supported("-Wno-sfinae-incomplete");
        })
    };
    builder.build();
}
