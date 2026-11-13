import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A profile on the Profile screen: a radio (base) or a tick (add-on), its
// name, size and what it brings. The whole card is the control.
QQC2.AbstractButton {
    id: card

    property bool radio: false
    property bool active: false
    property string title
    property string count

    hoverEnabled: true
    padding: Kirigami.Units.largeSpacing
    Accessible.role: radio ? Accessible.RadioButton : Accessible.CheckBox
    Accessible.name: title
    Accessible.description: text
    Accessible.checkable: true
    Accessible.checked: active

    background: Rectangle {
        radius: Theme.radius
        color: card.active ? Qt.tint(Theme.surface, Qt.alpha(Theme.accent, 0.16)) : card.hovered ? Theme.surfaceAlt : Theme.surface
        border.color: card.active ? Theme.accent : card.activeFocus ? Theme.textMuted : Theme.border
        border.width: card.active ? 2 : 1
        Behavior on color {
            ColorAnimation {
                duration: 120
            }
        }
    }
    contentItem: RowLayout {
        spacing: Kirigami.Units.mediumSpacing
        CheckMark {
            Layout.alignment: Qt.AlignTop
            mode: card.active ? "on" : "off"
            radio: card.radio
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            RowLayout {
                spacing: Kirigami.Units.smallSpacing
                QQC2.Label {
                    text: card.title
                    color: Theme.text
                    font.weight: Font.DemiBold
                }
                QQC2.Label {
                    text: "· " + card.count
                    color: Theme.textMuted
                    font.pointSize: Theme.smallSize
                }
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: card.text
                wrapMode: Text.Wrap
                color: Theme.textMuted
            }
        }
    }
}
