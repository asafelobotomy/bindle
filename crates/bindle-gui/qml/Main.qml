import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.bindle.wizard

QQC2.ApplicationWindow {
    id: root

    // 0 Welcome, 1 Profile, 2 Choose, 3 Review, 4 Apply.
    property int page: 0
    readonly property var apply: JSON.parse(backend.applyJson || '{"state": "idle"}')
    readonly property bool running: apply.state === "running"
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

    // Off uQ, Theme takes the desktop's colours from Kirigami, which the
    // desktop style also draws controls with.
    Item {
        id: winColors
        Kirigami.Theme.colorSet: Kirigami.Theme.Window
        Kirigami.Theme.inherit: false
        Item {
            id: viewColors
            Kirigami.Theme.colorSet: Kirigami.Theme.View
            Kirigami.Theme.inherit: false
        }
    }
    Component.onCompleted: {
        Theme.win = winColors.Kirigami.Theme;
        Theme.view = viewColors.Kirigami.Theme;
    }

    function go(to) {
        root.footerNote = "";
        if (to === 3)
            backend.refreshReview();
        root.page = Math.max(0, Math.min(4, to));
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
        ApplyPage {
            apply: root.apply
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
                text: root.page === 4 ? "Back to review" : "Back"
                visible: root.page > 0 && !root.running && root.apply.state !== "done"
                onClicked: root.go(root.page === 4 ? 3 : root.page - 1)
            }
            ActionButton {
                text: "Restart now"
                visible: root.page === 4 && root.apply.state === "done" && root.apply.needsReboot
                onClicked: backend.reboot()
            }
            QQC2.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                text: root.running ? "Keep the computer on until this finishes." : (backend.notice !== "" && root.page === 4 ? backend.notice : root.footerNote)
                color: Theme.textMuted
                elide: Text.ElideLeft
            }
            ActionButton {
                visible: !root.running
                primary: true
                text: {
                    if (root.page < 4)
                        return root.nextLabels[root.page];
                    if (root.apply.state === "done")
                        return "Close";
                    return root.apply.noAgent ? "Continue in a terminal" : "Try again";
                }
                onClicked: {
                    if (root.page < 3) {
                        root.go(root.page + 1);
                    } else if (root.page === 3 || (root.page === 4 && root.apply.state !== "done" && !root.apply.noAgent)) {
                        root.go(4);
                        backend.startApply();
                    } else if (root.apply.noAgent) {
                        backend.continueInTerminal();
                    } else {
                        Qt.quit();
                    }
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
            { page: 3, name: "5-review" },
            // Only with --sandbox: apply for real (recording commands) and wait.
            { page: 4, name: "6-applied", apply: true }
        ]
        property bool waiting: false

        interval: 600
        repeat: true
        running: backend.screenshotDir !== ""
        onTriggered: {
            if (waiting && root.running)
                return;
            waiting = false;
            if (step > 0)
                backend.grabWindow(backend.screenshotDir + "/" + plan[step - 1].name + ".png");
            if (step === plan.length) {
                Qt.quit();
                return;
            }
            const s = plan[step];
            if (s.apply && backend.sandbox === "") {
                Qt.quit();
                return;
            }
            root.go(s.page);
            if (s.apply) {
                backend.startApply();
                waiting = true;
            }
            if (s.category !== undefined)
                choose.show(s.category, s.module === undefined ? -1 : s.module);
            step += 1;
        }
    }
}
