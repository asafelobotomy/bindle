import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.bindle.wizard

// The update window (`bindle-gui --upgrade`, `bindleupgrade --gui`): the
// wizard's look and parts over bindleupgrade's plan. Check lists what each
// stage would do, with a switch where there is a choice; Update runs it and
// ends on the results.
QQC2.ApplicationWindow {
    id: root

    // 0 Check, 1 Update (running, then the results).
    property int page: 0
    readonly property var check: JSON.parse(backend.checkJson || '{"state": "checking"}')
    readonly property var run: JSON.parse(backend.runJson || '{"state": "idle"}')
    readonly property bool checking: check.state === "checking"
    readonly property bool running: run.state === "running"
    readonly property bool ended: ["done", "failed", "error"].includes(run.state)

    title: "Bindle · System update"
    visible: true
    width: Kirigami.Units.gridUnit * 56
    height: Kirigami.Units.gridUnit * 36
    minimumWidth: Kirigami.Units.gridUnit * 36
    minimumHeight: Kirigami.Units.gridUnit * 26
    color: Theme.bg
    font.family: Theme.fontUi !== "" ? Theme.fontUi : Qt.application.font.family
    font.pointSize: Theme.bodySize

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

    UpgradeBackend {
        id: backend
    }

    Binding {
        target: Theme
        property: "tokens"
        value: JSON.parse(backend.themeJson || '{"present": false}')
    }

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
        if (backend.screenshotDir === "")
            backend.startCheck();
    }

    function primaryLabel() {
        if (root.page === 0) {
            if (root.check.state === "error")
                return "Check again";
            return (root.check.work || 0) > 0 ? "Update now" : "Close";
        }
        return "Close";
    }
    function primary() {
        if (root.page === 0 && root.check.state === "error") {
            backend.startCheck();
        } else if (root.page === 0 && (root.check.work || 0) > 0) {
            root.page = 1;
            backend.startRun();
        } else {
            Qt.quit();
        }
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
                text: "System update"
                color: Theme.textMuted
            }
            Pill {
                visible: backend.dryRun || backend.sandbox !== "" || backend.screenshotDir !== ""
                text: "rehearsal: nothing changes"
                tone: "warning"
            }
            Item {
                Layout.fillWidth: true
            }
            StepBar {
                steps: ["Check", "Update", "Done"]
                current: root.page === 0 ? 0 : root.ended ? 2 : 1
                reached: current
                locked: true
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

        UpgradeCheckPage {
            id: checkPage
            check: root.check
            onToggled: id => backend.toggle(id)
            onOpenLink: url => Qt.openUrlExternally(url)
            onOpenBindle: backend.openBindle()
        }
        UpgradeRunPage {
            id: runPage
            run: root.run
            onRestart: backend.reboot()
            onTerminal: onlyAur => backend.openTerminal(onlyAur)
            onOpenLog: path => Qt.openUrlExternally("file://" + path)
            onOpenBindle: backend.openBindle()
            onReopen: backend.reopen()
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

            QQC2.Label {
                Layout.fillWidth: true
                text: root.running ? "Keep the computer on until this finishes."
                    : backend.notice !== "" ? backend.notice
                    : root.page === 0 && root.check.state === "ready"
                        ? ((root.check.work || 0) === 0 ? "Everything is up to date."
                            : root.check.work + (root.check.work === 1 ? " stage has" : " stages have") + " work to do.")
                    : ""
                color: Theme.textMuted
                elide: Text.ElideRight
            }
            ActionButton {
                visible: !root.running && !root.checking
                primary: true
                text: root.primaryLabel()
                onClicked: root.primary()
            }
        }
    }

    // `--screenshots <dir>`: the sample system, each page saved, then quit.
    Timer {
        id: shots

        property int step: 0
        readonly property var plan: [
            { name: "u1-checking" },
            { name: "u2-check", check: true },
            { name: "u2b-check-details", details: true },
            // The news read, then a dry run of the sample.
            { name: "u3-done", run: true },
            { name: "u3b-done-log", log: true },
            // A laptop where the update must wait, and a restart that isn't safe.
            { name: "u4-check-held", sample: "held" },
            { name: "u5-done-held", run: true, keepNews: true }
        ]
        property bool waiting: false

        interval: 600
        repeat: true
        running: backend.screenshotDir !== ""
        onTriggered: {
            if (waiting && (root.running || root.run.state === "idle"))
                return;
            waiting = false;
            if (step > 0)
                backend.grabWindow(backend.screenshotDir + "/" + plan[step - 1].name + ".png");
            if (step === plan.length) {
                Qt.quit();
                return;
            }
            const s = plan[step];
            if (s.check)
                backend.startCheck();
            if (s.details)
                checkPage.openAll = true;
            if (s.sample) {
                backend.loadSample(s.sample);
                runPage.logOpen = false;
                root.page = 0;
            }
            if (s.run) {
                if (!s.keepNews)
                    backend.toggle("news");
                root.page = 1;
                backend.startRun();
                waiting = true;
            }
            if (s.log)
                runPage.logOpen = true;
            step += 1;
        }
    }
}
