// History: the snapshot Bindle takes before each apply, and Undo for any of them. Undo shows
// what it would put back first (uQ's confirmation sheet); packages stay installed.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: tab

    property var host
    readonly property var view: tab.host ? tab.host.history : null
    readonly property bool ready: tab.view !== null && tab.view.problem === ""
    readonly property bool idle: !!tab.host && !tab.host.busy

    spacing: 16

    ErrorCard {
        title: "Couldn't read Bindle's history"
        detail: tab.view ? tab.view.problem : ""
        visible: tab.view !== null && tab.view.problem !== ""
        Layout.fillWidth: true
    }

    SettingsCard {
        title: "What Bindle changed"
        iconName: "view-history"
        help: "Bindle saves a snapshot of the files it is about to change. Undo puts them back; packages stay installed, and the command to remove them is shown."
        wide: true
        Layout.fillWidth: true

        SettingRow {
            settingId: "history"
            label: tab.view === null ? "Reading Bindle's history…" : (tab.ready ? (tab.view.snapshots.length === 0 ? "Bindle hasn't changed anything on this system yet." : Logic.plural(tab.view.snapshots.length, "snapshot", "snapshots")) : "History")
            highlight: tab.host ? tab.host.highlight : ""
            Layout.fillWidth: true
        }
        QQC2.ProgressBar {
            visible: tab.view === null
            indeterminate: true
            Layout.fillWidth: true
        }
        Repeater {
            model: tab.ready ? tab.view.snapshots : []

            ColumnLayout {
                id: row

                required property var modelData

                spacing: 2
                Layout.fillWidth: true

                RowLayout {
                    spacing: 12
                    Layout.fillWidth: true

                    ColumnLayout {
                        spacing: 0
                        Layout.fillWidth: true

                        UqLabel {
                            text: row.modelData.label !== "" ? row.modelData.label : row.modelData.id
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                        UqLabel {
                            text: row.modelData.created + " · " + Logic.snapshotLine(row.modelData) + (row.modelData.undone ? " · undone" : "")
                            opacity: 0.7
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }
                    QQC2.Button {
                        text: row.modelData.undone ? "Undo again…" : "Undo…"
                        icon.name: "edit-undo"
                        enabled: tab.idle
                        onClicked: tab.host.write("snap:" + row.modelData.id, "undo", [row.modelData.id], "@output")
                    }
                }
                Outcome {
                    host: tab.host
                    noteKey: "snap:" + row.modelData.id
                    Layout.fillWidth: true
                }
            }
        }
    }
}
