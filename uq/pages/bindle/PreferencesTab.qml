// Preferences: bindleUP's settings, read with `bindle config show` (followed from the first time the
// tab opens) and changed one key at a time with `config set`. Bindle checks every value before it
// writes, so a refusal is shown under its row and nothing changes. A value set for the whole
// system says so; setting it here overrides it for you, and Reset goes back.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: tab

    property var host
    property var view: null
    property var watching: null
    readonly property bool ready: tab.view !== null && tab.view.problem === ""
    readonly property bool idle: !!tab.host && !tab.host.busy

    function show(result) {
        const next = Logic.configView(result);
        if (next !== null)
            tab.view = next;
    }

    function autoload() {
        if (tab.visible && tab.host && tab.host.ctx && tab.watching === null)
            tab.watching = tab.host.ctx.command.watch("config", tab.show);
    }

    function save(key, value) {
        tab.host.write("pref:" + key, "config-set", [key, String(value)]);
    }

    function reset(key) {
        tab.host.write("pref:" + key, "config-reset", [key]);
    }

    onVisibleChanged: tab.autoload()

    Connections {
        target: tab.host
        function onCtxChanged() {
            tab.autoload();
        }
    }

    spacing: 16

    ErrorCard {
        title: "Couldn't read the update preferences"
        detail: tab.view ? tab.view.problem : ""
        visible: tab.view !== null && tab.view.problem !== ""
        Layout.fillWidth: true
    }

    SettingsCard {
        title: "Background checks"
        iconName: "chronometer"
        help: "A timer checks for updates every day, and after you log in. When there is something to update, a notification offers to open the update window."
        visible: tab.ready
        Layout.fillWidth: true

        SwitchRow {
            settingId: "update-check"
            label: "Check for updates every day"
            highlight: tab.host ? tab.host.highlight : ""
            value: tab.ready ? tab.view.notify : false
            enabled: tab.idle
            onChosen: value => tab.save("update_notify", value ? "on" : "off")
            Layout.fillWidth: true
        }
        Outcome {
            host: tab.host
            noteKey: "pref:update_notify"
            Layout.fillWidth: true
        }
    }

    Repeater {
        model: tab.ready ? tab.view.groups : []

        SettingsCard {
            id: group

            required property var modelData

            title: group.modelData.title
            iconName: group.modelData.title === "Free space" ? "drive-harddisk" : (group.modelData.title === "Battery" ? "battery" : (group.modelData.title === "Apps" ? "applications-all" : "preferences-system-time"))
            Layout.fillWidth: true

            Repeater {
                model: group.modelData.keys

                ColumnLayout {
                    id: pref

                    required property var modelData

                    spacing: 2
                    Layout.fillWidth: true

                    SettingRow {
                        settingId: pref.modelData.key === "space.root_mb" ? "space" : (pref.modelData.key === "battery_min_percent" ? "battery" : (pref.modelData.key === "releases.follow" ? "follow" : pref.modelData.key))
                        label: pref.modelData.label + (pref.modelData.unit !== "" && pref.modelData.kind === "int" ? " (" + pref.modelData.unit + ")" : "")
                        highlight: tab.host ? tab.host.highlight : ""
                        Layout.fillWidth: true

                        HelpTip {
                            tip: pref.modelData.help
                        }
                        QQC2.SpinBox {
                            id: spin
                            visible: pref.modelData.kind === "int"
                            from: pref.modelData.min
                            to: pref.modelData.max
                            editable: true
                            enabled: tab.idle
                            value: pref.modelData.kind === "int" ? pref.modelData.value : 0
                            // Saved when the person stops changing it, so stepping doesn't write every step.
                            onValueModified: timer.restart()

                            Timer {
                                id: timer
                                interval: 800
                                onTriggered: {
                                    if (spin.value !== pref.modelData.value)
                                        tab.save(pref.modelData.key, spin.value);
                                    spin.value = Qt.binding(() => pref.modelData.kind === "int" ? pref.modelData.value : 0);
                                }
                            }
                        }
                        UqComboBox {
                            id: combo
                            visible: pref.modelData.kind === "choice"
                            model: Logic.FOLLOW_NAMES
                            implicitWidth: 260
                            enabled: tab.idle
                            currentIndex: Logic.indexOfKey(Logic.FOLLOW_KEYS, pref.modelData.value, 0)
                            onActivated: index => {
                                combo.currentIndex = Qt.binding(() => Logic.indexOfKey(Logic.FOLLOW_KEYS, pref.modelData.value, 0));
                                tab.save(pref.modelData.key, Logic.FOLLOW_KEYS[index]);
                            }
                        }
                        QQC2.Button {
                            text: "Reset"
                            icon.name: "edit-undo"
                            visible: pref.modelData.source === "user"
                            enabled: tab.idle
                            onClicked: tab.reset(pref.modelData.key)
                        }
                    }
                    UqLabel {
                        visible: Logic.sourceNote(pref.modelData) !== ""
                        text: Logic.sourceNote(pref.modelData)
                        opacity: 0.7
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Outcome {
                        host: tab.host
                        noteKey: "pref:" + pref.modelData.key
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }

    SettingsCard {
        title: "Apps with their own setting"
        iconName: "applications-all"
        help: "Where one pinned app updates from, over the setting above. Add one with: bindle config set releases.apps.<app> bindle"
        visible: tab.ready && tab.view.apps.length > 0
        Layout.fillWidth: true

        Repeater {
            model: tab.ready ? tab.view.apps : []

            ColumnLayout {
                id: app

                required property var modelData

                spacing: 2
                Layout.fillWidth: true

                SettingRow {
                    label: app.modelData.app
                    Layout.fillWidth: true

                    UqComboBox {
                        id: appCombo
                        model: Logic.FOLLOW_NAMES
                        implicitWidth: 260
                        enabled: tab.idle
                        currentIndex: Logic.indexOfKey(Logic.FOLLOW_KEYS, app.modelData.value, 0)
                        onActivated: index => {
                            appCombo.currentIndex = Qt.binding(() => Logic.indexOfKey(Logic.FOLLOW_KEYS, app.modelData.value, 0));
                            tab.save("releases.apps." + app.modelData.app, Logic.FOLLOW_KEYS[index]);
                        }
                    }
                    QQC2.Button {
                        text: "Reset"
                        icon.name: "edit-undo"
                        visible: app.modelData.source === "user"
                        enabled: tab.idle
                        onClicked: tab.reset("releases.apps." + app.modelData.app)
                    }
                }
                Outcome {
                    host: tab.host
                    noteKey: "pref:releases.apps." + app.modelData.app
                    Layout.fillWidth: true
                }
            }
        }
    }

    SettingsCard {
        title: "More in Bindle"
        iconName: "bindle"
        help: "Picking apps, choosing what's new item by item and the other recovery tools are in Bindle's own window."
        Layout.fillWidth: true

        QQC2.Button {
            text: "Open Bindle"
            icon.name: "window-new"
            enabled: tab.idle
            onClicked: tab.host.write("more", "open", [], "Bindle is opening in its own window.")
        }
        Outcome {
            host: tab.host
            noteKey: "more"
            Layout.fillWidth: true
        }
    }
}
