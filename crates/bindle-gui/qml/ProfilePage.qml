import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A base (Minimal, Desktop) and any add-ons on top (Gaming, Developer), each
// naming what it brings. Changing the base after editing asks first.
QQC2.ScrollView {
    id: page

    // Wizard::profiles: {bases, addons, base, edited}.
    property var profiles: ({})
    signal chosen(string id)
    signal addon(string id)

    readonly property var bases: profiles.bases || []
    readonly property var addons: profiles.addons || []
    // A base waiting for "Replace my picks".
    property string pending

    function pick(id) {
        if (id === profiles.base)
            return;
        if (profiles.edited) {
            pending = id;
            return;
        }
        chosen(id);
    }
    // "Steam, Wine, GameMode and 4 more".
    function preview(names, n) {
        const shown = names.slice(0, n).join(", ");
        return names.length > n ? shown + " and " + (names.length - n) + " more" : shown;
    }

    contentWidth: availableWidth

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.mediumSpacing

        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            QQC2.Label {
                font.pointSize: Theme.headingSize
                font.weight: Font.DemiBold
                text: "Choose a starting point"
                color: Theme.text
            }
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "Pick a base, then add what you use this computer for. Everything stays editable on the next screens."
                color: Theme.textMuted
            }
        }

        QQC2.Label {
            text: "1 · Base"
            color: Theme.textMuted
            font.weight: Font.DemiBold
        }
        GridLayout {
            Layout.fillWidth: true
            columns: page.availableWidth > Kirigami.Units.gridUnit * 30 ? 2 : 1
            columnSpacing: Kirigami.Units.mediumSpacing
            rowSpacing: Kirigami.Units.mediumSpacing
            Repeater {
                model: page.bases
                ProfileCard {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    radio: true
                    active: modelData.id === page.profiles.base
                    title: modelData.name
                    count: modelData.count + " features"
                    text: modelData.description
                    onClicked: page.pick(modelData.id)
                }
            }
        }

        Notice {
            Layout.fillWidth: true
            visible: page.pending !== ""
            tone: "warning"
            icon: "dialog-warning"
            text: "You've changed some picks. Switching the base starts over from its suggestions."
            Row {
                spacing: Kirigami.Units.smallSpacing
                ActionButton {
                    text: "Keep my picks"
                    onClicked: page.pending = ""
                }
                ActionButton {
                    text: "Start over"
                    onClicked: {
                        page.chosen(page.pending);
                        page.pending = "";
                    }
                }
            }
        }

        QQC2.Label {
            Layout.topMargin: Kirigami.Units.smallSpacing
            text: "2 · Add on top, any or none"
            color: Theme.textMuted
            font.weight: Font.DemiBold
        }
        GridLayout {
            Layout.fillWidth: true
            columns: page.availableWidth > Kirigami.Units.gridUnit * 30 ? 2 : 1
            columnSpacing: Kirigami.Units.mediumSpacing
            rowSpacing: Kirigami.Units.mediumSpacing
            Repeater {
                model: page.addons
                ProfileCard {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    active: modelData.on === true
                    title: modelData.name
                    count: "adds " + modelData.count
                    text: page.preview(modelData.names, 7)
                    onClicked: page.addon(modelData.id)
                }
            }
        }
    }
}
