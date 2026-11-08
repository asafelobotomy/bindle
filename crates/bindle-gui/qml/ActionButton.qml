import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// A footer button; `primary` is the one accent-filled action per screen.
QQC2.Button {
    id: button

    property bool primary: false

    leftPadding: Kirigami.Units.largeSpacing
    rightPadding: Kirigami.Units.largeSpacing
    contentItem: QQC2.Label {
        text: button.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        color: button.primary ? Theme.accentText : Theme.text
        font.weight: button.primary ? Font.DemiBold : Font.Normal
    }
    background: Rectangle {
        implicitHeight: Kirigami.Units.gridUnit * 1.8
        radius: Theme.radius
        color: button.primary ? (button.down ? Qt.darker(Theme.accent, 1.15) : Theme.accent)
            : (button.down ? Theme.overlay : button.hovered ? Theme.surfaceAlt : Theme.surface)
        border.color: button.primary ? Theme.accent : Theme.border
        border.width: 1
    }
}
