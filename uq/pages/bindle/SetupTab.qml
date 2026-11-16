// Setup: what Bindle last applied for you, and what of it isn't in place any more. The check
// runs when the page opens; "Pick apps again" opens Bindle's own window, where apps are chosen.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: tab

    property var host
    readonly property var view: tab.host ? tab.host.setup : null
    readonly property bool ready: tab.view !== null && tab.view.problem === ""

    property bool showSteps: false

    spacing: 16

    ErrorCard {
        title: "Couldn't look at this system"
        detail: tab.view ? tab.view.problem : ""
        visible: tab.view !== null && tab.view.problem !== ""
        Layout.fillWidth: true
    }

    SettingsCard {
        title: "Your setup"
        iconName: "system-software-install"
        help: "Bindle set up this system's apps and settings. Pick apps again to add or remove some."
        Layout.fillWidth: true
        actions: [
            QQC2.Button {
                text: "Pick apps again"
                icon.name: "list-add"
                highlighted: tab.host && tab.host.highlight === "pick-apps"
                enabled: !!tab.host && !tab.host.busy
                onClicked: tab.host.write("setup", "open", [], "Bindle is opening in its own window.", null)
            }
        ]

        SettingRow {
            settingId: "pick-apps"
            label: !tab.ready ? "Looking at this system…" : (tab.view.has ? "Profile: " + (tab.view.profile !== "" ? tab.view.profile : "none") : "Bindle hasn't set up anything for you yet.")
            highlight: tab.host ? tab.host.highlight : ""
            Layout.fillWidth: true
        }
        QQC2.ProgressBar {
            visible: tab.view === null
            indeterminate: true
            Layout.fillWidth: true
        }
        UqLabel {
            visible: tab.ready && tab.view.lastApplied > 0
            text: tab.ready ? "Last applied " + Logic.when(tab.view.lastApplied) : ""
            opacity: 0.7
            Layout.fillWidth: true
        }
        UqLabel {
            visible: tab.ready && tab.view.with.length > 0
            text: tab.ready ? "Added to the profile: " + tab.view.with.join(", ") : ""
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        UqLabel {
            visible: tab.ready && tab.view.without.length > 0
            text: tab.ready ? "Taken out of the profile: " + tab.view.without.join(", ") : ""
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        Outcome {
            host: tab.host
            noteKey: "setup"
            Layout.fillWidth: true
        }
    }

    SettingsCard {
        title: "Missing pieces"
        iconName: "dialog-warning"
        help: "What Bindle would do again if you applied your setup once more: things it installed that have gone missing."
        visible: tab.ready && tab.view.has
        Layout.fillWidth: true
        actions: [
            QQC2.Button {
                text: "Check again"
                icon.name: "view-refresh"
                highlighted: tab.host && tab.host.highlight === "missing"
                enabled: !!tab.host && !tab.host.busy
                onClicked: tab.host.loadSetup()
            }
        ]

        SettingRow {
            settingId: "missing"
            label: !tab.ready ? "" : (tab.view.error !== "" ? "The saved setup can't be checked" : (tab.view.missing.length === 0 ? "Everything Bindle set up is still in place." : Logic.plural(tab.view.missing.length, "step", "steps") + " would run again"))
            highlight: tab.host ? tab.host.highlight : ""
            Layout.fillWidth: true
        }
        UqLabel {
            visible: tab.ready && tab.view.error !== ""
            text: tab.ready ? tab.view.error : ""
            color: tab.host ? tab.host.errorColor() : "red"
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        Repeater {
            model: tab.ready && tab.view.error === "" ? tab.view.groups : []

            ColumnLayout {
                required property var modelData

                spacing: 2
                Layout.fillWidth: true

                UqLabel {
                    text: parent.modelData.label
                    font.bold: true
                    Layout.fillWidth: true
                }
                UqLabel {
                    visible: parent.modelData.detail !== ""
                    text: parent.modelData.detail
                    opacity: 0.7
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }
        }
        QQC2.Button {
            visible: tab.ready && tab.view.error === "" && tab.view.missing.length > 0
            text: tab.showSteps ? "Hide every step" : "Show every step"
            flat: true
            onClicked: tab.showSteps = !tab.showSteps
        }
        Repeater {
            model: tab.showSteps && tab.ready && tab.view.error === "" ? tab.view.missing : []

            UqLabel {
                required property string modelData

                text: "• " + modelData
                opacity: 0.7
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
        }
        UqLabel {
            visible: tab.ready && tab.view.missing.length > 0
            text: "Pick apps again and finish in Bindle's window to put them back."
            opacity: 0.7
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
    }

    SettingsCard {
        title: "More software"
        iconName: "system-software-install"
        help: "bindaru finds, installs and removes software from the Arch repositories and the AUR, and shows you every AUR recipe before it is built. System upgrades stay in bindleUP, under Updates."
        Layout.fillWidth: true
        actions: [
            QQC2.Button {
                objectName: "open-bindaru"
                text: "Open bindaru"
                icon.name: "system-software-install"
                highlighted: tab.host && tab.host.highlight === "open-bindaru"
                enabled: !!tab.host
                onClicked: tab.host.openApp("bindaru", "io.github.asafelobotomy.bindaru.desktop", "bindaru is opening in its own window.")
            }
        ]

        SettingRow {
            settingId: "open-bindaru"
            label: "Software store"
            highlight: tab.host ? tab.host.highlight : ""
            Layout.fillWidth: true
        }
        Outcome {
            host: tab.host
            noteKey: "bindaru"
            Layout.fillWidth: true
        }
    }
}
