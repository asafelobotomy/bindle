import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// The selected module: what it is, whether it applies here, and what it
// installs and changes (present::module_details).
Panel {
    id: panel

    property var module: null
    property bool selected: false
    property string neededBy

    QQC2.Label {
        anchors.centerIn: parent
        width: parent.width
        visible: panel.module === null
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: "Point at anything to see what it installs and changes."
        color: Theme.textMuted
    }

    QQC2.ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: parent.width
            spacing: Kirigami.Units.smallSpacing
            visible: panel.module !== null

            QQC2.Label {
                Layout.fillWidth: true
                text: panel.module ? panel.module.name : ""
                wrapMode: Text.Wrap
                color: Theme.text
                font.weight: Font.DemiBold
            }
            Flow {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                visible: panel.module !== null
                Pill {
                    visible: panel.selected
                    text: "Selected"
                    tone: "accent"
                    filled: true
                }
                Pill {
                    visible: !panel.selected && panel.neededBy !== ""
                    text: "Needed by " + panel.neededBy
                    tone: "accent"
                }
                Pill {
                    visible: panel.module !== null && !!panel.module.risk
                    text: panel.module && panel.module.risk === "advanced" ? "Advanced" : "Moderate"
                    tone: panel.module && panel.module.risk === "advanced" ? "error" : "warning"
                }
                Pill {
                    visible: panel.module !== null && !!panel.module.source
                    text: panel.module ? "From " + panel.module.source : ""
                }
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: panel.module ? panel.module.description : ""
                wrapMode: Text.Wrap
                color: Theme.text
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: panel.module !== null && !!panel.module.provided
                text: panel.module && panel.module.provided ? "Already set up: " + panel.module.provided : ""
                wrapMode: Text.Wrap
                color: Theme.success
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: panel.module !== null && !panel.module.provided && !!panel.module.unavailable
                text: panel.module && panel.module.unavailable ? "Not available here: " + panel.module.unavailable : ""
                wrapMode: Text.Wrap
                color: Theme.error
            }
            Repeater {
                model: panel.module ? panel.module.notes : []
                QQC2.Label {
                    required property string modelData
                    Layout.fillWidth: true
                    text: "• " + modelData
                    wrapMode: Text.Wrap
                    color: Theme.accent
                }
            }
            Repeater {
                model: panel.module ? panel.module.details : []
                ColumnLayout {
                    required property string modelData
                    readonly property int colon: modelData.indexOf(": ")
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    spacing: 0
                    QQC2.Label {
                        text: colon > 0 ? modelData.slice(0, colon) : ""
                        visible: colon > 0
                        color: Theme.textMuted
                        font.pointSize: Theme.smallSize
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: colon > 0 ? modelData.slice(colon + 2) : modelData
                        wrapMode: Text.Wrap
                        color: Theme.text
                        font.family: Theme.fontMono
                        font.pointSize: Theme.smallSize
                    }
                }
            }
            QQC2.Label {
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.smallSpacing
                visible: panel.module !== null && panel.module.requires !== ""
                text: panel.module ? "Adds with it: " + panel.module.requires : ""
                wrapMode: Text.Wrap
                color: Theme.textMuted
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: panel.module !== null && panel.module.conflicts !== ""
                text: panel.module ? "Choose one: can't be installed with " + panel.module.conflicts : ""
                wrapMode: Text.Wrap
                color: Theme.warning
            }
        }
    }
}
