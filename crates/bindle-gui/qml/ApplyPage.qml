import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// The plan running (crate::apply::ApplyState): the current step large, with
// the steps and the log below. Done: a result banner, actions (restart,
// undo, the log) and a few things to try first; the steps and log fold away.
ColumnLayout {
    id: page

    property var apply: ({ state: "idle", steps: [], log: [] })
    // The review the run came from: its counts and category groups.
    property var review: ({})
    property alias detailsOpen: details.open
    signal restart
    signal undo
    signal openLog(string path)

    readonly property var steps: apply.steps || []
    readonly property int finished: steps.filter(s => s.status === "done").length
    readonly property int failedAt: steps.findIndex(s => s.status === "failed")
    readonly property string state: apply.state || "idle"
    readonly property bool done: state === "done"
    readonly property bool ended: done || state === "failed" || state === "error"
    readonly property int features: (review.counts || {}).modules || 0
    readonly property var current: steps.find(s => s.status === "running") || null
    // Apps the run added, to point at: the categories that hold apps.
    readonly property var apps: (review.groups || [])
        .filter(g => ["internet", "creative", "gaming", "ai", "development"].includes(g.id))
        .reduce((all, g) => all.concat(g.modules), [])

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

    // While running: the step under way, large.
    QQC2.Label {
        Layout.fillWidth: true
        visible: page.state === "running" && page.current !== null
        text: page.current ? page.current.text : ""
        elide: Text.ElideRight
        color: Theme.text
        font.pointSize: Math.round(Theme.bodySize * 1.2)
        font.family: Theme.fontMono
    }

    // Done: what to do next.
    Flow {
        Layout.fillWidth: true
        visible: page.done
        spacing: Kirigami.Units.mediumSpacing
        Repeater {
            model: [
                ["Undo these changes", "Opens a terminal that puts the changed files back.", page.apply.snapshot ? "undo" : ""],
                ["Open the log", (page.apply.logPath || "").split("/").pop(), page.apply.logPath ? "log" : ""],
                ["Run Bindle again later", "Your picks are kept, and it only does what's new.", ""]
            ]
            QQC2.AbstractButton {
                id: act
                required property var modelData
                width: Math.max(Kirigami.Units.gridUnit * 13, (page.width - 2 * Kirigami.Units.mediumSpacing) / 3 - 1)
                enabled: modelData[2] !== ""
                visible: modelData[0] !== "Undo these changes" || modelData[2] !== ""
                hoverEnabled: true
                padding: Kirigami.Units.mediumSpacing
                onClicked: modelData[2] === "undo" ? page.undo() : page.openLog(page.apply.logPath)
                Accessible.role: Accessible.Button
                Accessible.name: modelData[0]
                background: Rectangle {
                    radius: Theme.radius
                    color: act.hovered && act.enabled ? Theme.surfaceAlt : Theme.surface
                    border.color: Theme.border
                }
                contentItem: ColumnLayout {
                    spacing: 2
                    QQC2.Label {
                        text: act.modelData[0]
                        color: Theme.text
                        font.weight: Font.DemiBold
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: act.modelData[1]
                        wrapMode: Text.Wrap
                        color: Theme.textMuted
                        font.pointSize: Theme.smallSize
                    }
                }
            }
        }
    }

    Panel {
        Layout.fillWidth: true
        visible: page.done && page.apps.length > 0
        implicitHeight: tips.implicitHeight + 2 * padding
        ColumnLayout {
            id: tips
            width: parent.width
            spacing: Kirigami.Units.smallSpacing
            QQC2.Label {
                text: "Try these first"
                color: Theme.text
                font.weight: Font.DemiBold
            }
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: {
                    const n = page.apps.length;
                    const names = n > 6 ? page.apps.slice(0, 6).join(", ") + " and " + (n - 6) + " more"
                        : n > 1 ? page.apps.slice(0, -1).join(", ") + " and " + page.apps[n - 1] : page.apps.join("");
                    return names + (n === 1 ? " is" : " are") + " in your app launcher.";
                }
                color: Theme.textMuted
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
        visible: page.ended && !page.done && page.state !== "error"
        wrapMode: Text.Wrap
        color: Theme.textMuted
        text: page.apply.logPath ? "Log: " + page.apply.logPath : ""
    }
}
