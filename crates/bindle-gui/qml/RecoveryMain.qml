import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.bindle.wizard

// The Recovery window (`bindle-gui --recovery`, `bindle rollback --gui`):
// Roll back, Rebase and Factory Restore, each explained (what stays, what
// goes) and confirmed here, then run by the same `bindle` commands as in a
// terminal, their output shown as they go.
ThemedWindow {
    id: root

    // 0 the three tools, 1 Roll back, 2 Rebase, 3 Factory Restore, 4 the run.
    property int page: 0
    property int chosen: -1
    property string confirming: ""
    readonly property var status: JSON.parse(backend.statusJson || "{}")
    readonly property var snaps: JSON.parse(backend.snapshotsJson || '{"state": "none"}')
    readonly property var run: JSON.parse(backend.runJson || '{"state": "idle"}')
    readonly property bool running: run.state === "running"
    readonly property bool rehearsal: backend.dryRun || backend.sandbox !== "" || backend.screenshotDir !== ""

    title: "Bindle · Recovery"
    width: Kirigami.Units.gridUnit * 52
    height: Kirigami.Units.gridUnit * 36
    minimumWidth: Kirigami.Units.gridUnit * 34
    minimumHeight: Kirigami.Units.gridUnit * 26
    themeBackend: backend

    RecoveryBackend {
        id: backend
    }

    function go(kind) {
        root.confirming = "";
        root.page = 4;
        backend.start(kind, root.chosen);
    }

    readonly property var tools: [
        { page: 1, icon: "edit-undo", title: "Roll back",
          text: "Go back to how the system was at one of its snapshots: before an update, or a day ago.",
          stays: "Your home folder: documents, settings, downloads.",
          goes: "System changes since that snapshot (they're kept aside, to go back to).",
          why: status.rollback || "" },
        { page: 2, icon: "view-refresh", title: "Rebase",
          text: "Put the system back to how it was installed, Bindle's setup included.",
          stays: "Your home folder and every app you added yourself.",
          goes: "Changes to the system's own apps and settings (your copies are kept).",
          why: "" },
        { page: 3, icon: "edit-delete", title: "Factory Restore",
          text: "Erase this computer and install it from new, as on the first day.",
          stays: "Nothing on the disk you erase.",
          goes: "Everything: your files, apps and settings. Back up first.",
          why: status.restore || "" }
    ]

    header: HeaderBar {
        QQC2.Label {
            text: "Recovery"
            color: Theme.textMuted
        }
        Pill {
            visible: root.rehearsal
            text: "rehearsal: nothing changes"
            tone: "warning"
        }
        Item {
            Layout.fillWidth: true
        }
    }

    StackLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        currentIndex: root.page

        // 0: the three tools.
        QQC2.ScrollView {
            id: home
            contentWidth: availableWidth
            ColumnLayout {
                width: home.availableWidth
                spacing: Kirigami.Units.largeSpacing
                QQC2.Label {
                    text: "Something isn't right?"
                    font.pointSize: Theme.headingSize
                    font.weight: Font.DemiBold
                    color: Theme.text
                }
                Repeater {
                    model: root.tools
                    Panel {
                        id: card
                        required property var modelData
                        Layout.fillWidth: true
                        // Its own width, not its text's: the button stays inside.
                        Layout.preferredWidth: home.availableWidth
                        implicitWidth: home.availableWidth
                        implicitHeight: cardBody.implicitHeight + 2 * padding
                        edge: modelData.page === 3 ? Theme.error : Theme.border
                        RowLayout {
                            id: cardBody
                            width: parent.width
                            spacing: Kirigami.Units.largeSpacing
                            Glyph {
                                Layout.alignment: Qt.AlignTop
                                name: card.modelData.icon
                                tint: card.modelData.page === 3 ? Theme.error : Theme.accent
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: Kirigami.Units.smallSpacing
                                QQC2.Label { text: card.modelData.title; font.weight: Font.DemiBold; color: Theme.text }
                                QQC2.Label { Layout.fillWidth: true; wrapMode: Text.Wrap; text: card.modelData.text; color: Theme.text }
                                QQC2.Label { Layout.fillWidth: true; wrapMode: Text.Wrap; text: "Stays: " + card.modelData.stays; color: Theme.textMuted; font.pointSize: Theme.smallSize }
                                QQC2.Label { Layout.fillWidth: true; wrapMode: Text.Wrap; text: "Goes: " + card.modelData.goes; color: Theme.textMuted; font.pointSize: Theme.smallSize }
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    visible: card.modelData.why !== ""
                                    wrapMode: Text.Wrap
                                    text: "Not here: " + card.modelData.why
                                    color: Theme.warning
                                    font.pointSize: Theme.smallSize
                                }
                            }
                            ActionButton {
                                Layout.alignment: Qt.AlignVCenter
                                enabled: card.modelData.why === ""
                                text: card.modelData.title + "…"
                                onClicked: {
                                    root.page = card.modelData.page;
                                    if (card.modelData.page === 1 && root.snaps.state === "none")
                                        backend.loadSnapshots();
                                }
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.mediumSpacing
                    QQC2.Switch {
                        id: keep
                        Layout.alignment: Qt.AlignTop
                        enabled: !root.running && (root.status.restore || "") === ""
                        checked: root.status.imageKept === true
                        onToggled: root.go(checked ? "keep-on" : "keep-off")
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.text
                        text: "Keep the installer on this computer (1.5 GB): Factory Restore without the internet, and \"Roll back\" in the boot menu for when the system won't start."
                        MouseArea {
                            anchors.fill: parent
                            enabled: keep.enabled
                            onClicked: keep.toggle(), keep.toggled()
                        }
                    }
                }
            }
        }

        // 1: Roll back.
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "Pick a snapshot. It becomes the system from the next start; your home folder stays as it is. The system as it is now is kept aside: `bindo rollback-cleanup` deletes it once you're happy."
                color: Theme.text
            }
            Notice {
                Layout.fillWidth: true
                visible: root.snaps.error !== undefined
                tone: "error"
                icon: "dialog-error"
                text: root.snaps.error || ""
            }
            QQC2.BusyIndicator {
                visible: root.snaps.state === "loading"
                running: visible
            }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: Array.isArray(root.snaps) ? root.snaps : []
                delegate: QQC2.RadioDelegate {
                    required property var modelData
                    width: ListView.view.width
                    checked: root.chosen === modelData.number
                    text: modelData.number + "   " + (modelData.date || "").slice(0, 16) + "   " + (modelData.description || "(no description)")
                        + (modelData.type === "pre" ? "  (before)" : modelData.type === "post" ? "  (after)" : "")
                    onClicked: root.chosen = modelData.number
                }
            }
        }

        // 2: Rebase.
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            Repeater {
                model: [
                    "Reinstalls the apps the system was installed with, and the ones Bindle set up; puts back those that were removed.",
                    "Puts their settings files back to how they come (your changed copies are kept in /var/lib/bindle/rebase).",
                    "Applies Bindle's setup again.",
                    "Leaves your home folder, the apps you added yourself, Flatpaks, and this computer's own settings: accounts, disks, its name, language, start-up."
                ]
                QQC2.Label {
                    required property string modelData
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: "• " + modelData
                    color: Theme.text
                }
            }
            Notice {
                Layout.fillWidth: true
                tone: "accent"
                icon: "dialog-information"
                text: "A snapshot is taken first where snapper is set up, so Roll back can undo the rebase."
            }
            Item { Layout.fillHeight: true }
        }

        // 3: Factory Restore.
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            Notice {
                Layout.fillWidth: true
                tone: "error"
                icon: "dialog-warning"
                text: "Factory Restore erases this computer. Back up your files first: nothing on the erased disk can be brought back."
            }
            Repeater {
                model: [
                    "The computer restarts into the Arch Linux installer, with Bindle.",
                    "You choose the disk to erase (this computer's own is picked) and set it up again: desktop, user, apps.",
                    "Nothing is erased until the installer's last question; restarting before then comes back here."
                ]
                QQC2.Label {
                    required property string modelData
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: "• " + modelData
                    color: Theme.text
                }
            }
            QQC2.Label { text: "Type ERASE to go on:"; color: Theme.text }
            QQC2.TextField {
                id: erase
                Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                placeholderText: "ERASE"
            }
            Item { Layout.fillHeight: true }
        }

        // 4: the run.
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                font.weight: Font.DemiBold
                color: root.run.state === "failed" ? Theme.error : Theme.text
                text: root.running ? "Working… keep the computer on."
                    : root.run.state === "failed" ? "It didn't finish: what it printed is below. Nothing after the failed step ran."
                    : root.run.restart ? "Done. Restart to start " + (root.run.kind === "restore" ? "the installer." : "the rolled-back system.")
                    : "Done."
            }
            LogView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                lines: root.run.log || []
            }
        }
    }

    footer: WindowBar {
        footer: true

        ActionButton {
            visible: root.page > 0 && !root.running
            text: "Back"
            onClicked: {
                root.confirming = "";
                root.page = 0;
            }
        }
        QQC2.Label {
            Layout.fillWidth: true
            text: backend.notice !== "" ? backend.notice
                : root.confirming !== "" ? "Click again to confirm."
                : ""
            color: root.confirming !== "" ? Theme.warning : Theme.textMuted
            elide: Text.ElideRight
        }
        ActionButton {
            visible: !root.running
            primary: true
            enabled: root.page === 1 ? root.chosen >= 0 : root.page === 3 ? erase.text === "ERASE" : true
            text: root.page === 1 ? (root.confirming === "rollback" ? "Yes, roll back to " + root.chosen : "Roll back to " + (root.chosen >= 0 ? root.chosen : "…"))
                : root.page === 2 ? (root.confirming === "rebase" ? "Yes, rebase" : "Rebase now")
                : root.page === 3 ? "Restart into the installer"
                : root.page === 4 && root.run.restart && root.run.state === "done" ? "Restart now"
                : "Close"
            onClicked: {
                if (root.page === 1 || root.page === 2) {
                    const kind = root.page === 1 ? "rollback" : "rebase";
                    if (root.confirming === kind)
                        root.go(kind);
                    else
                        root.confirming = kind;
                } else if (root.page === 3) {
                    root.go("restore");
                } else if (root.page === 4 && root.run.restart && root.run.state === "done") {
                    backend.reboot();
                } else {
                    Qt.quit();
                }
            }
        }
    }

    // `--screenshots <dir>`: each page, a sample rollback run, then quit.
    ShotDriver {
        appBackend: backend
        plan: [
            { name: "r1-recovery", page: 0 },
            { name: "r2-rollback", page: 1, list: true },
            { name: "r3-rebase", page: 2 },
            { name: "r4-restore", page: 3 },
            { name: "r5-rolled-back", run: true }
        ]
        onShoot: s => {
            if (s.list) {
                backend.loadSnapshots();
                root.chosen = 41;
            }
            if (s.page !== undefined)
                root.page = s.page;
            if (s.page === 3)
                erase.text = "ERASE";
            if (s.run)
                root.go("rollback");
        }
    }
}
