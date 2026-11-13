import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.bindle.wizard

QQC2.ApplicationWindow {
    id: root

    // 0 Welcome, 1 Profile, 2 Choose (one category at a time), 3 Summary,
    // 4 Review, 5 Apply.
    property int page: 0
    readonly property var apply: JSON.parse(backend.applyJson || '{"state": "idle"}')
    readonly property bool running: apply.state === "running"
    property string footerNote: ""
    readonly property var steps: ["Welcome", "Profile", "Choose", "Summary", "Review", "Apply"]
    readonly property var catalog: JSON.parse(backend.catalogJson || "[]")
    readonly property var selection: JSON.parse(backend.stateJson || "{}")
    readonly property int picked: selection.total || 0

    title: "Bindle"
    visible: true
    width: Kirigami.Units.gridUnit * 60
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
        Theme.backend = backend;
    }

    function go(to) {
        root.footerNote = "";
        if (to === 4)
            backend.refreshReview();
        root.page = Math.max(0, Math.min(5, to));
    }
    // Where the primary button leads, as its label.
    function nextLabel() {
        switch (root.page) {
        case 0:
            return "Get started";
        case 1:
            return "Continue";
        case 2:
            return choose.last ? "See your selections" : "Next: " + choose.nextName;
        case 3:
            return "Go to final review";
        case 4:
            return "Apply changes";
        }
        if (root.apply.state === "done")
            return "Close";
        return root.apply.noAgent ? "Continue in a terminal" : "Try again";
    }
    function next() {
        if (root.page === 1) {
            choose.open(0);
            root.go(2);
        } else if (root.page === 2 && !choose.last) {
            choose.open(choose.categoryIndex + 1);
        } else if (root.page < 4) {
            root.go(root.page + 1);
        } else if (root.page === 4 || (root.apply.state !== "done" && !root.apply.noAgent)) {
            root.go(5);
            backend.startApply();
        } else if (root.apply.noAgent) {
            backend.continueInTerminal();
        } else {
            Qt.quit();
        }
    }
    function back() {
        if (root.page === 2 && choose.categoryIndex > 0 && choose.query === "")
            choose.open(choose.categoryIndex - 1);
        else
            root.go(root.page - 1);
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
            categories: root.catalog
            selection: root.selection
            feedback: JSON.parse(backend.feedbackJson || "{}")
            onToggled: id => backend.toggle(id)
        }
        SummaryPage {
            categories: root.catalog
            selection: root.selection
            onEdit: index => {
                choose.open(index);
                root.go(2);
            }
            onConfirmed: root.go(4)
        }
        ReviewPage {
            id: reviewPage
            review: JSON.parse(backend.reviewJson || "{}")
        }
        ApplyPage {
            id: applyPage
            apply: root.apply
            review: reviewPage.review
            onRestart: backend.reboot()
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
                text: root.page === 5 ? "Back to review" : "Back"
                visible: root.page > 0 && !root.running && root.apply.state !== "done"
                onClicked: root.back()
            }
            QQC2.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                text: root.running ? "Keep the computer on until this finishes." : (backend.notice !== "" && root.page === 5 ? backend.notice
                    : root.page === 2 || root.page === 3 ? root.picked + (root.picked === 1 ? " feature selected" : " features selected")
                    : root.footerNote)
                color: Theme.textMuted
                elide: Text.ElideLeft
            }
            ActionButton {
                visible: !root.running
                primary: true
                text: root.nextLabel()
                onClicked: root.next()
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
            { page: 2, name: "3-choose-grid", category: 0, view: "grid" },
            // A category with "choose one" sets, in each view.
            { page: 2, name: "3b-choose-one", category: 2, view: "grid", toggle: "earlyoom" },
            { page: 2, name: "3c-choose-list", category: 4, view: "list", toggle: "brave" },
            { page: 2, name: "3d-choose-compact", category: 6, view: "compact" },
            { page: 2, name: "3e-choose-selected", category: 1, view: "grid", filter: "selected" },
            { page: 3, name: "4-summary" },
            { page: 4, name: "5-review" },
            { page: 4, name: "5b-review-details", details: true },
            // Only with --sandbox: apply for real (recording commands) and wait.
            { page: 5, name: "6-applied", apply: true },
            { page: 5, name: "6b-applied-details", details: true }
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
            if (s.details) {
                // Open the current page's details, scrolled to show them.
                if (s.page === 4) {
                    reviewPage.detailsOpen = true;
                    reviewPage.showDetails();
                } else {
                    applyPage.detailsOpen = true;
                }
            } else {
                root.go(s.page);
            }
            if (s.apply) {
                backend.startApply();
                waiting = true;
            }
            if (s.category !== undefined) {
                choose.open(s.category);
                choose.view = s.view;
                if (s.toggle)
                    backend.toggle(s.toggle);
                choose.setFilter(s.filter || "all");
                choose.moduleId = s.toggle || "";
            }
            step += 1;
        }
    }
}
