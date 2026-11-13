import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// A rounded toggle for filters and the view switch: filled while `active`.
// `count` trails the label in a quieter colour.
QQC2.AbstractButton {
    id: chip

    property bool active: false
    property string count

    leftPadding: Kirigami.Units.mediumSpacing
    rightPadding: Kirigami.Units.mediumSpacing
    topPadding: Kirigami.Units.smallSpacing
    bottomPadding: Kirigami.Units.smallSpacing
    Accessible.role: Accessible.CheckBox
    Accessible.name: text
    Accessible.checkable: true
    Accessible.checked: active

    background: Rectangle {
        radius: height / 2
        color: chip.active ? Theme.accent : chip.hovered ? Theme.surfaceAlt : Theme.surface
        border.color: chip.active ? Theme.accent : chip.activeFocus ? Theme.textMuted : Theme.border
        border.width: 1
        Behavior on color {
            ColorAnimation {
                duration: 100
            }
        }
    }
    contentItem: Row {
        spacing: Kirigami.Units.smallSpacing
        QQC2.Label {
            text: chip.text
            color: chip.active ? Theme.accentText : Theme.text
            font.pointSize: Theme.smallSize
        }
        QQC2.Label {
            visible: chip.count !== ""
            text: chip.count
            color: chip.active ? Theme.accentText : Theme.textMuted
            font.pointSize: Theme.smallSize
        }
    }
}
