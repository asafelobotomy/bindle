import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// The plan running (crate::apply::ApplyState), then how it ended. Done shows
// a result banner, what to do next and the features by category; the steps
// and log fold away under "Steps and log".
ColumnLayout {
    id: page

    property var apply: ({ state: "idle", steps: [], log: [] })
    // The review the run came from: its counts and category groups.
    property var review: ({})
    property alias detailsOpen: details.open
    signal restart

    readonly property var steps: apply.steps || []
    readonly property int finished: steps.filter(s => s.status === "done").length
    readonly property int failedAt: steps.findIndex(s => s.status === "failed")
    readonly property string state: apply.state || "idle"
    readonly property bool done: state === "done"
    readonly property bool ended: done || state === "failed" || state === "error"
    readonly property int features: (review.counts || {}).modules || 0

    spacing: Kirigami.Units.mediumSpacing

    // Result banner.
    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.mediumSpacing
        Glyph {
            visible: page.ended && status === Image.Ready
            name: page.done ? "dialog-ok-apply" : "dialog-error"
            tint: page.done ? Theme.success : Theme.error
            size: Kirigami.Units.iconSizes.medium
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0
            QQC2.Label {
                Layout.fillWidth: true
                font.pointSize: Theme.headingSize
                font.weight: Font.DemiBold
                color: page.done ? Theme.success : page.state === "running" ? Theme.text : Theme.error
                text: page.done ? "Your system is set up"
                    : page.state === "failed" ? "Stopped at step " + (page.failedAt + 1)
                    : page.state === "error" ? "Nothing was changed"
                    : "Applying your changes"
            }
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.textMuted
                text: {
                    if (page.state === "running")
                        return "Step " + Math.min(page.finished + 1, page.steps.length) + " of " + page.steps.length
                            + (page.apply.snapshot ? " · a snapshot was saved, so this run can be undone" : "");
                    if (page.done)
                        return (page.features === 1 ? "1 feature" : page.features + " features") + " added in "
                            + page.steps.length + " steps.";
                    if (page.state === "failed")
                        return "Nothing after it ran. Every step is safe to repeat: fix the cause, then try again.";
                    return "";
                }
            }
        }
    }

    Panel {
        Layout.fillWidth: true
        visible: page.state === "failed" || page.state === "error"
        edge: Theme.error
        implicitHeight: problem.implicitHeight + 2 * padding
        QQC2.Label {
            id: problem
            width: parent.width
            wrapMode: Text.Wrap
            color: Theme.text
            text: page.apply.noAgent
                ? "Bindle can't show a password prompt here because no polkit agent is running. Continue in a terminal, where it asks for your password instead."
                : (page.apply.error || "")
        }
    }

    // What to do next.
    Notice {
        Layout.fillWidth: true
        visible: page.done && (page.apply.needsReboot || page.apply.relogin)
        tone: "accent"
        icon: page.apply.needsReboot ? "system-reboot" : "system-log-out"
        text: page.apply.needsReboot ? "Restart to load the new kernel, microcode or drivers."
            : "Log out and back in for the new group memberships."
        ActionButton {
            visible: page.apply.needsReboot === true
            text: "Restart now"
            onClicked: page.restart()
        }
    }

    QQC2.ProgressBar {
        Layout.fillWidth: true
        visible: page.steps.length > 0 && !page.done && page.state !== "error"
        from: 0
        to: Math.max(1, page.steps.length)
        value: page.finished
    }

    // What was set up, by category.
    GridLayout {
        Layout.fillWidth: true
        visible: page.done
        columns: Math.max(1, Math.floor(page.width / (Kirigami.Units.gridUnit * 14)))
        columnSpacing: Kirigami.Units.mediumSpacing
        rowSpacing: Kirigami.Units.mediumSpacing
        Repeater {
            model: page.done ? (page.review.groups || []) : []
            CategoryCard {
                required property var modelData
                Layout.fillWidth: true
                Layout.preferredWidth: Kirigami.Units.gridUnit * 13
                categoryId: modelData.id
                name: modelData.name
                modules: modelData.modules
                done: true
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        visible: page.done
        Disclosure {
            id: details
            text: "Steps and log"
        }
        Item {
            Layout.fillWidth: true
        }
    }

    // Each step's status beside the command output: always while running or
    // after a failure, on request once done.
    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: !page.done || details.open
        spacing: Kirigami.Units.largeSpacing

        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 3
            clip: true
            model: page.steps
            spacing: 2
            delegate: RowLayout {
                id: stepRow
                required property var modelData
                width: ListView.view.width
                spacing: Kirigami.Units.mediumSpacing
                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit
                    horizontalAlignment: Text.AlignHCenter
                    text: stepRow.modelData.status === "done" ? "✓"
                        : stepRow.modelData.status === "failed" ? "✗"
                        : stepRow.modelData.status === "running" ? "●" : "○"
                    color: stepRow.modelData.status === "done" ? Theme.success
                        : stepRow.modelData.status === "failed" ? Theme.error
                        : stepRow.modelData.status === "running" ? Theme.accent : Theme.textMuted
                }
                QQC2.Label {
                    id: stepText
                    Layout.fillWidth: true
                    text: stepRow.modelData.text
                    elide: Text.ElideRight
                    color: stepRow.modelData.status === "pending" ? Theme.textMuted : Theme.text
                    QQC2.ToolTip.visible: stepHover.hovered && stepText.truncated
                    QQC2.ToolTip.text: stepRow.modelData.text
                    HoverHandler {
                        id: stepHover
                    }
                }
            }
        }

        Panel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 2
            visible: (page.apply.log || []).length > 0
            color: Theme.surfaceAlt
            ListView {
                id: log
                anchors.fill: parent
                clip: true
                model: page.apply.log || []
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

    // Keeps the Done summary at the top while the steps are folded.
    Item {
        Layout.fillHeight: true
        visible: page.done && !details.open
    }

    QQC2.Label {
        Layout.fillWidth: true
        visible: page.ended && page.state !== "error"
        wrapMode: Text.Wrap
        color: Theme.textMuted
        text: (page.apply.snapshot ? "Changed your mind? Run \"bindle undo\" in a terminal to put the changed files back. " : "")
            + (page.apply.logPath ? "Log: " + page.apply.logPath : "")
    }
}
