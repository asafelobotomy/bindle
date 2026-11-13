import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// The update running (crate::upgrade::run::RunState): each stage with its
// status beside the command output. Once it ends: a result banner, what to
// do next (restart, the AUR or a retry in a terminal, the log) and the
// stage table; the output folds away.
ColumnLayout {
    id: page

    property var run: ({ state: "idle", stages: [], log: [] })
    property alias logOpen: logToggle.open
    signal restart
    signal terminal(bool onlyAur)
    signal openLog(string path)
    signal openBindle
    signal reopen

    readonly property var stages: run.stages || []
    readonly property string state: run.state || "idle"
    readonly property bool ended: ["done", "failed", "error"].includes(state)
    readonly property int finished: stages.filter(s => !["pending", "running"].includes(s.status)).length
    readonly property var current: stages.find(s => s.status === "running") || null
    readonly property int failures: stages.filter(s => s.status === "failed").length

    spacing: Kirigami.Units.mediumSpacing

    function mark(status) {
        return { done: "✓", current: "✓", ran: "✓", note: "!", skipped: "–", held: "‖", failed: "✗", running: "●" }[status] || "○";
    }
    function ink(status) {
        return ["done", "current", "ran"].includes(status) ? Theme.success : status === "failed" ? Theme.error
            : status === "held" || status === "note" ? Theme.warning : status === "running" ? Theme.accent : Theme.textMuted;
    }
    function word(status) {
        return { done: page.run.rehearsal ? "rehearsed" : "updated", current: "up to date", ran: "finished", note: "note",
            skipped: "skipped", held: "held", failed: "failed", running: "running", pending: "waiting" }[status] || status;
    }

    // Result banner.
    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.mediumSpacing
        Glyph {
            visible: page.ended && status === Image.Ready
            name: page.state === "done" ? "dialog-ok-apply" : "dialog-error"
            tint: page.state === "done" ? Theme.success : Theme.error
            size: Kirigami.Units.iconSizes.medium
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0
            QQC2.Label {
                Layout.fillWidth: true
                font.pointSize: Theme.headingSize
                font.weight: Font.DemiBold
                color: page.state === "done" ? Theme.success : page.ended ? Theme.error : Theme.text
                text: page.state === "done" ? (page.run.rehearsal ? "Rehearsal finished" : "Your system is up to date")
                    : page.state === "failed" ? (page.failures === 1 ? "One stage failed" : page.failures + " stages failed")
                    : page.state === "error" ? "Nothing was changed" : "Updating your system"
            }
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.textMuted
                text: page.state === "running" ? "Stage " + Math.min(page.finished + 1, page.stages.length) + " of " + page.stages.length
                    : page.run.rehearsal && page.ended ? "A dry run: every command was printed or recorded, nothing changed."
                    : page.state === "failed" ? "The other stages still ran. Each one is safe to repeat."
                    : page.state === "error" ? (page.run.error || "") : ""
            }
        }
    }

    QQC2.ProgressBar {
        Layout.fillWidth: true
        visible: page.state === "running"
        from: 0
        to: Math.max(1, page.stages.length)
        value: page.finished
    }
    QQC2.Label {
        Layout.fillWidth: true
        visible: page.current !== null
        text: page.current ? page.current.title + ": " + page.current.summary : ""
        elide: Text.ElideRight
        color: Theme.text
        font.pointSize: Math.round(Theme.bodySize * 1.2)
    }

    // What to do next.
    Notice {
        Layout.fillWidth: true
        visible: page.ended && page.run.relaunch === true
        tone: "accent"
        icon: "system-software-update"
        text: "Bindle updated itself. The new version finishes the update, with its own pins and checks."
        ActionButton {
            text: "Go on"
            onClicked: page.reopen()
        }
    }
    Notice {
        Layout.fillWidth: true
        visible: page.ended && !!page.run.restartBlocked
        tone: "error"
        icon: "dialog-warning"
        text: (page.run.restartBlocked || "").replace(/^don't restart yet: /, "Don't restart yet: ") + "."
    }
    Notice {
        Layout.fillWidth: true
        visible: page.ended && !!page.run.reboot && !page.run.restartBlocked
        tone: "accent"
        icon: "system-reboot"
        text: "Restart: " + (page.run.reboot || "").replace(/^[a-z ]+: /, "") + "."
        ActionButton {
            text: "Restart now"
            onClicked: page.restart()
        }
    }
    Notice {
        Layout.fillWidth: true
        visible: page.ended && page.run.retryTerminal === true
        tone: "error"
        icon: "dialog-scripts"
        text: page.run.noAgent ? "No password prompt could open here. Finish in a terminal, where pacman asks for it."
            : "The system upgrade needs an answer pacman can only ask in a terminal."
        ActionButton {
            text: "Finish in a terminal"
            onClicked: page.terminal(false)
        }
    }
    Notice {
        Layout.fillWidth: true
        visible: page.ended && (page.run.newFeatures || 0) > 0 && !page.run.relaunch
        tone: "accent"
        icon: "list-add"
        text: "What's new in Bindle: " + page.run.newFeatures + (page.run.newFeatures === 1 ? " item" : " items")
            + " to review (new features, changes, clean-ups). Nothing changes unless you pick it."
        ActionButton {
            text: "What's new"
            onClicked: page.openBindle()
        }
    }
    Notice {
        Layout.fillWidth: true
        visible: page.ended && page.run.aurTerminal === true && page.run.retryTerminal !== true
        tone: "muted"
        icon: "dialog-scripts"
        text: "AUR packages update in a terminal, where paru shows what it builds and asks for your password."
        ActionButton {
            text: "Update the AUR"
            onClicked: page.terminal(true)
        }
    }

    // Every stage: its status beside the output, which folds away at the end.
    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: Kirigami.Units.largeSpacing

        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 3
            clip: true
            model: page.stages
            spacing: 2
            delegate: RowLayout {
                id: stageRow
                required property var modelData
                width: ListView.view.width
                spacing: Kirigami.Units.mediumSpacing
                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit
                    horizontalAlignment: Text.AlignHCenter
                    text: page.mark(stageRow.modelData.status)
                    color: page.ink(stageRow.modelData.status)
                }
                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 7.5
                    text: stageRow.modelData.title
                    color: stageRow.modelData.status === "pending" ? Theme.textMuted : Theme.text
                    font.weight: Font.DemiBold
                }
                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                    visible: page.ended
                    text: page.word(stageRow.modelData.status)
                    color: page.ink(stageRow.modelData.status)
                }
                QQC2.Label {
                    id: what
                    Layout.fillWidth: true
                    text: stageRow.modelData.detail || stageRow.modelData.summary
                    elide: Text.ElideRight
                    color: Theme.textMuted
                    QQC2.ToolTip.visible: hover.hovered && what.truncated
                    QQC2.ToolTip.text: what.text
                    HoverHandler {
                        id: hover
                    }
                }
            }
        }

        Panel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 2
            visible: (page.run.log || []).length > 0 && (!page.ended || logToggle.open)
            color: Theme.surfaceAlt
            ListView {
                anchors.fill: parent
                clip: true
                model: page.run.log || []
                onCountChanged: positionViewAtEnd()
                delegate: QQC2.Label {
                    required property string modelData
                    width: ListView.view.width
                    text: modelData
                    wrapMode: Text.WrapAnywhere
                    color: Theme.textMuted
                    font.family: Theme.fontMono
                    font.pointSize: Theme.smallSize
                }
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        visible: page.ended && page.state !== "error"
        Disclosure {
            id: logToggle
            text: "Output"
        }
        Item {
            Layout.fillWidth: true
        }
        ActionButton {
            visible: !!page.run.logPath
            text: "Open the log"
            onClicked: page.openLog(page.run.logPath)
        }
    }
}
