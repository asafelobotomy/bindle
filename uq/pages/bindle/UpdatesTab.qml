// Updates (bindleUP): what an update would do, stage by stage, then Update now with progress.
// The look at the system runs when the tab is first shown (it only reads, and needs no password);
// the update asks first and shows everything it would change (uQ's confirmation sheet).
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: tab

    property var host
    property bool started: false
    readonly property var groups: Logic.planGroups(tab.host ? tab.host.plan : null)
    readonly property bool idle: !!tab.host && !tab.host.busy

    /// The first time the tab is shown, also when Settings opens the page on it.
    function autostart() {
        if (tab.visible && !tab.started && tab.host && tab.host.ctx) {
            tab.started = true;
            tab.host.check();
        }
    }

    onVisibleChanged: tab.autostart()

    Connections {
        target: tab.host
        function onCtxChanged() {
            tab.autostart();
        }
    }

    spacing: 16

    SettingsCard {
        title: "System updates"
        iconName: "system-software-update"
        help: "bindleUP updates Bindle, packages, the AUR, Flatpak and more in one run. It asks for your password when it needs it, and holds the package stages back when the Arch news asks for a manual step."
        wide: true
        Layout.fillWidth: true
        actions: [
            QQC2.Button {
                text: "Check again"
                icon.name: "view-refresh"
                enabled: tab.idle
                onClicked: tab.host.check()
            },
            QQC2.Button {
                objectName: "update-now"
                text: "Update now"
                icon.name: "system-software-update"
                highlighted: true
                enabled: tab.idle && tab.groups.work.length > 0
                onClicked: tab.host.update("update", [])
            }
        ]

        SettingRow {
            settingId: "updates"
            label: tab.host && tab.host.plan !== null ? (tab.groups.work.length === 0 ? "Everything is up to date." : Logic.plural(tab.groups.work.length, "stage has", "stages have") + " updates") : "System updates"
            highlight: tab.host ? tab.host.highlight : ""
            Layout.fillWidth: true
        }
        ProgressView {
            host: tab.host
            progressKey: "check"
            Layout.fillWidth: true
        }
        ProgressView {
            host: tab.host
            progressKey: "update"
            Layout.fillWidth: true
        }
        Outcome {
            host: tab.host
            noteKey: "check"
            Layout.fillWidth: true
        }
        Outcome {
            host: tab.host
            noteKey: "update"
            Layout.fillWidth: true
        }
        UqLabel {
            visible: tab.host && tab.host.plan !== null && tab.groups.current > 0
            text: tab.groups.current + " other " + (tab.groups.current === 1 ? "stage is" : "stages are") + " up to date."
            opacity: 0.7
            Layout.fillWidth: true
        }
        UqLabel {
            text: "A bigger update than ten minutes of work, or an AUR update, belongs in a terminal: run bindleup there."
            opacity: 0.7
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
    }

    SettingsCard {
        title: "Last update"
        iconName: "dialog-ok"
        visible: !!tab.host && tab.host.lastRun !== null
        wide: true
        Layout.fillWidth: true

        UqLabel {
            text: tab.host && tab.host.lastRun !== null ? Logic.progressLine(tab.host.lastRun) : ""
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        Repeater {
            model: tab.host && tab.host.lastRun !== null ? tab.host.lastRun.results : []

            UqLabel {
                id: line

                required property var modelData

                text: line.modelData.title + ": " + Logic.outcomeWord(line.modelData.outcome) + (line.modelData.why !== "" ? " (" + line.modelData.why + ")" : "")
                wrapMode: Text.Wrap
                color: line.modelData.outcome === "failed" && tab.host ? tab.host.errorColor() : (tab.host ? tab.host.textColor() : "black")
                opacity: line.modelData.outcome === "failed" ? 1 : 0.8
                Layout.fillWidth: true
            }
        }
    }

    Repeater {
        model: tab.groups.work

        SettingsCard {
            id: card

            required property var modelData

            title: card.modelData.title
            iconName: "software-update-available"
            help: card.modelData.needsRoot ? "This stage asks for your password." : ""
            wide: true
            Layout.fillWidth: true
            actions: [
                QQC2.Button {
                    text: "Only this"
                    icon.name: "media-playback-start"
                    enabled: tab.idle
                    onClicked: tab.host.update("update-only", [card.modelData.id])
                }
            ]

            UqLabel {
                text: card.modelData.summary
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Repeater {
                model: card.modelData.details

                UqLabel {
                    required property string modelData

                    text: modelData
                    opacity: 0.8
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }
            UqLabel {
                visible: card.modelData.more > 0
                text: "… and " + card.modelData.more + " more"
                opacity: 0.7
                Layout.fillWidth: true
            }
        }
    }

    SettingsCard {
        title: "Held back"
        iconName: "dialog-warning"
        help: "These would run, but something has to be taken care of first."
        visible: tab.groups.held.length > 0
        wide: true
        Layout.fillWidth: true

        Repeater {
            model: tab.groups.held

            ColumnLayout {
                required property var modelData

                spacing: 0
                Layout.fillWidth: true

                UqLabel {
                    text: modelData.title
                    Layout.fillWidth: true
                }
                UqLabel {
                    text: modelData.why
                    color: tab.host ? tab.host.errorColor() : "red"
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }
        }
    }

    SettingsCard {
        title: "To know"
        iconName: "dialog-information"
        visible: tab.groups.notes.length > 0
        wide: true
        Layout.fillWidth: true

        Repeater {
            model: tab.groups.notes

            ColumnLayout {
                required property var modelData

                spacing: 0
                Layout.fillWidth: true

                UqLabel {
                    text: modelData.title + ": " + modelData.summary
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
                Repeater {
                    model: modelData.details

                    UqLabel {
                        required property string modelData

                        text: modelData
                        opacity: 0.8
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }
}
