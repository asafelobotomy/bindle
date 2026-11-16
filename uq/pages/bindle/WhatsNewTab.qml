// What's new: what this Bindle would add, change or clean up compared with what earlier applies
// set up here. Applying takes the items Bindle ticks by default, after a preview and with a
// snapshot first (History undoes it); picking items one by one is in Bindle's own window.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: tab

    property var host
    readonly property var view: tab.host ? tab.host.whatsNew : null
    readonly property bool ready: tab.view !== null && tab.view.problem === ""
    readonly property bool idle: !!tab.host && !tab.host.busy

    spacing: 16

    ErrorCard {
        title: "Couldn't read what's new"
        detail: tab.view ? tab.view.problem : ""
        visible: tab.view !== null && tab.view.problem !== ""
        Layout.fillWidth: true
    }

    SettingsCard {
        title: tab.ready && tab.view.version !== "" ? "What's new in Bindle " + tab.view.version : "What's new"
        iconName: "view-list-details"
        help: "Compared with what earlier versions of Bindle set up on this system."
        wide: true
        Layout.fillWidth: true
        actions: [
            QQC2.Button {
                text: "Review in Bindle…"
                icon.name: "window-new"
                enabled: tab.idle
                onClicked: tab.host.write("whatsnew", "open", [], "Bindle is opening in its own window.")
            },
            QQC2.Button {
                objectName: "apply-whats-new"
                text: "Apply what's ticked…"
                icon.name: "dialog-ok-apply"
                highlighted: true
                enabled: tab.idle && tab.ready && tab.view.ticked > 0
                onClicked: tab.host.write("whatsnew", "apply-whats-new", [], "@output")
            }
        ]

        SettingRow {
            settingId: "whats-new"
            label: tab.view === null ? "Looking at what's new…" : (!tab.ready ? "What's new" : (tab.view.groups.length === 0 ? "Nothing new: this system has everything this Bindle would set up." : (tab.view.since !== "" ? "Since Bindle " + tab.view.since : "Since the earlier setup")))
            highlight: tab.host ? tab.host.highlight : ""
            Layout.fillWidth: true
        }
        QQC2.ProgressBar {
            visible: tab.view === null
            indeterminate: true
            Layout.fillWidth: true
        }
        UqLabel {
            visible: tab.ready && !tab.view.recorded
            text: "An older Bindle left no record, so this is compared with what is installed."
            opacity: 0.7
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        Outcome {
            host: tab.host
            noteKey: "whatsnew"
            Layout.fillWidth: true
        }
    }

    Repeater {
        model: tab.ready ? tab.view.groups : []

        SettingsCard {
            id: group

            required property var modelData

            title: group.modelData.title
            iconName: group.modelData.kind === "new" || group.modelData.kind === "optional" ? "list-add" : (group.modelData.kind === "updated" ? "view-refresh" : "edit-delete")
            wide: true
            Layout.fillWidth: true

            Repeater {
                model: group.modelData.items

                ColumnLayout {
                    id: item

                    required property var modelData

                    spacing: 0
                    Layout.fillWidth: true

                    UqLabel {
                        text: (item.modelData.ticked ? "✓ " : "○ ") + item.modelData.name
                        Layout.fillWidth: true
                    }
                    UqLabel {
                        visible: item.modelData.why !== ""
                        text: item.modelData.why
                        opacity: 0.7
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Repeater {
                        model: item.modelData.changes.slice(0, 4)

                        UqLabel {
                            required property string modelData

                            text: modelData
                            opacity: 0.8
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }
                    }
                    UqLabel {
                        visible: item.modelData.changes.length > 4
                        text: "… and " + (item.modelData.changes.length - 4) + " more"
                        opacity: 0.7
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }
}
