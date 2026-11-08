import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// The plan running (crate::apply::ApplyState), then how it ended.
ColumnLayout {
    id: page

    property var apply: ({ state: "idle", steps: [], log: [] })
    readonly property var steps: apply.steps || []
    readonly property int finished: steps.filter(s => s.status === "done").length
    readonly property int failedAt: steps.findIndex(s => s.status === "failed")
    readonly property string state: apply.state || "idle"

    spacing: Kirigami.Units.mediumSpacing

    QQC2.Label {
        Layout.fillWidth: true
        font.pointSize: Theme.headingSize
        font.weight: Font.DemiBold
        color: page.state === "done" ? Theme.success : page.state === "running" ? Theme.text : Theme.error
        text: page.state === "done" ? "Your system is set up"
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
            if (page.state === "done") {
                let s = "All " + page.steps.length + " steps finished.";
                if (page.apply.needsReboot)
                    s += " Restart to load the new kernel, microcode or drivers.";
                else if (page.apply.relogin)
                    s += " Log out and back in for the new group memberships.";
                return s;
            }
            if (page.state === "failed")
                return "Nothing after it ran. Every step is safe to repeat: fix the cause, then try again.";
            return "";
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

    QQC2.ProgressBar {
        Layout.fillWidth: true
        visible: page.steps.length > 0 && page.state !== "error"
        from: 0
        to: Math.max(1, page.steps.length)
        value: page.finished
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
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
                    Layout.fillWidth: true
                    text: stepRow.modelData.text
                    elide: Text.ElideRight
                    color: stepRow.modelData.status === "pending" ? Theme.textMuted : Theme.text
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

    QQC2.Label {
        Layout.fillWidth: true
        visible: page.state === "done" || page.state === "failed"
        wrapMode: Text.Wrap
        color: Theme.textMuted
        text: (page.apply.snapshot ? "Changed your mind? Run \"bindle undo\" in a terminal to put the changed files back. " : "")
            + (page.apply.logPath ? "Log: " + page.apply.logPath : "")
    }
}
