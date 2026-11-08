import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.bindle.wizard

QQC2.ApplicationWindow {
    id: root

    // 0 Welcome, 1 Profile, 2 Choose, 3 Review.
    property int page: 0
    property string footerNote: ""
    readonly property var steps: ["Welcome", "Profile", "Choose", "Review", "Apply"]
    readonly property var nextLabels: ["Get started", "Continue", "Review changes", "Apply changes"]

    title: "Bindle"
    visible: true
    width: Kirigami.Units.gridUnit * 54
    height: Kirigami.Units.gridUnit * 36
    minimumWidth: Kirigami.Units.gridUnit * 38
    minimumHeight: Kirigami.Units.gridUnit * 26
    color: Theme.bg
    font.family: Theme.fontUi !== "" ? Theme.fontUi : Qt.application.font.family
    font.pointSize: Theme.bodySize

    // Fusion and the Qt Quick controls draw from the palette: feed it the theme.
    palette {
        window: Theme.bg
        windowText: Theme.text
        base: Theme.surface
        alternateBase: Theme.surfaceAlt
        text: Theme.text
        button: Theme.surfaceAlt
        buttonText: Theme.text
        highlight: Theme.accent
        highlightedText: Theme.accentText
        placeholderText: Theme.textMuted
        mid: Theme.border
        dark: Theme.border
        light: Theme.overlay
        midlight: Theme.overlay
    }

    Backend {
        id: backend
    }

    Binding {
        target: Theme
        property: "tokens"
        value: JSON.parse(backend.themeJson || '{"present": false}')
    }

    function go(to) {
        root.footerNote = "";
        if (to === 3)
            backend.refreshReview();
        root.page = Math.max(0, Math.min(3, to));
    }

    header: Rectangle {
        implicitHeight: headerRow.implicitHeight + 2 * Kirigami.Units.mediumSpacing
        color: Theme.surface

        RowLayout {
            id: headerRow
            anchors.fill: parent
            anchors.leftMargin: Kirigami.Units.largeSpacing
            anchors.rightMargin: Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.mediumSpacing

            QQC2.Label {
                text: "Bindle"
                font.weight: Font.DemiBold
                color: Theme.accent
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: backend.header
                color: Theme.textMuted
                elide: Text.ElideRight
            }
            StepBar {
                steps: root.steps
                current: root.page
            }
        }

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.border
        }
    }

    StackLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        currentIndex: root.page

        WelcomePage {
            summary: JSON.parse(backend.welcomeJson || "{}")
        }
        ProfilePage {
            profiles: JSON.parse(backend.profilesJson || "[]")
            current: backend.profile
            onChosen: id => backend.chooseProfile(id)
        }
        ChoosePage {
            id: choose
            categories: JSON.parse(backend.categoriesJson || "[]")
            notice: backend.notice
            onToggled: id => backend.toggle(id)
        }
        ReviewPage {
            review: JSON.parse(backend.reviewJson || "{}")
        }
    }

    footer: Rectangle {
        implicitHeight: footerRow.implicitHeight + 2 * Kirigami.Units.mediumSpacing
        color: Theme.surface

        Rectangle {
            width: parent.width
            height: 1
            color: Theme.border
        }

        RowLayout {
            id: footerRow
            anchors.fill: parent
            anchors.leftMargin: Kirigami.Units.largeSpacing
            anchors.rightMargin: Kirigami.Units.largeSpacing

            ActionButton {
                text: "Back"
                visible: root.page > 0
                onClicked: root.go(root.page - 1)
            }
            QQC2.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                text: root.footerNote
                color: Theme.textMuted
                elide: Text.ElideLeft
            }
            ActionButton {
                text: root.nextLabels[root.page]
                primary: true
                onClicked: {
                    if (root.page < 3)
                        root.go(root.page + 1);
                    else
                        root.footerNote = "Applying from this window arrives in the next version.";
                }
            }
        }
    }

    // `--screenshots <dir>`: show each screen, save it, then quit.
    Timer {
        id: shots

        property int step: 0
        readonly property var plan: [
            { page: 0, name: "1-welcome" },
            { page: 1, name: "2-profile" },
            { page: 2, name: "3-choose", category: 0 },
            { page: 2, name: "4-choose-hardware", category: 1, module: 1 },
            { page: 3, name: "5-review" }
        ]

        interval: 600
        repeat: true
        running: backend.screenshotDir !== ""
        onTriggered: {
            if (step > 0)
                backend.grabWindow(backend.screenshotDir + "/" + plan[step - 1].name + ".png");
            if (step === plan.length) {
                Qt.quit();
                return;
            }
            const s = plan[step];
            root.go(s.page);
            if (s.category !== undefined)
                choose.show(s.category, s.module === undefined ? -1 : s.module);
            step += 1;
        }
    }
}
