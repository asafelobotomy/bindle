import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One of Review's three lists: a heading with its size, then each row's
// `what` over its `why`. Long lists show their first rows and a "Show all".
Panel {
    id: list

    property string title
    property string count
    property var rows: []
    property bool mono: false
    // Left alone: drawn on the page colour, so it reads as secondary.
    property bool quiet: false
    property string empty
    property bool all: false
    readonly property int limit: 8

    color: quiet ? Theme.bg : Theme.surface
    padding: Kirigami.Units.mediumSpacing
    implicitHeight: body.implicitHeight + 2 * padding

    ColumnLayout {
        id: body
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            spacing: Kirigami.Units.smallSpacing
            QQC2.Label {
                text: list.title
                color: Theme.text
                font.weight: Font.DemiBold
            }
            QQC2.Label {
                text: "· " + list.count
                color: Theme.textMuted
                font.pointSize: Theme.smallSize
            }
        }
        QQC2.Label {
            visible: list.rows.length === 0
            text: list.empty
            color: Theme.textMuted
            font.pointSize: Theme.smallSize
        }
        Repeater {
            model: list.all ? list.rows : list.rows.slice(0, list.limit)
            ColumnLayout {
                required property var modelData
                Layout.fillWidth: true
                spacing: 0
                Rectangle {
                    Layout.fillWidth: true
                    Layout.bottomMargin: Kirigami.Units.smallSpacing
                    height: 1
                    color: Theme.surfaceAlt
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: modelData.what
                    wrapMode: Text.WrapAnywhere
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    color: Theme.text
                    font.family: list.mono ? Theme.fontMono : Theme.fontUi !== "" ? Theme.fontUi : Qt.application.font.family
                    font.pointSize: list.mono ? Theme.smallSize : Theme.bodySize
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: modelData.why
                    wrapMode: Text.Wrap
                    color: Theme.textMuted
                    font.pointSize: Theme.smallSize
                }
            }
        }
        QQC2.AbstractButton {
            id: more
            visible: list.rows.length > list.limit
            hoverEnabled: true
            padding: 2
            onClicked: list.all = !list.all
            Accessible.role: Accessible.Button
            Accessible.name: contentItem.text
            contentItem: QQC2.Label {
                text: list.all ? "Show fewer" : "Show all " + list.rows.length
                color: Theme.accent
                font.underline: more.hovered
                font.pointSize: Theme.smallSize
            }
        }
    }
}
