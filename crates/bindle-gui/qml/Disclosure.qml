import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A "Technical details" style toggle: a chevron and a label; `open` flips.
QQC2.ItemDelegate {
    id: toggle

    property bool open: false

    padding: Kirigami.Units.smallSpacing
    onClicked: open = !open
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.description: open ? "Expanded" : "Collapsed"

    background: Rectangle {
        radius: Theme.radius
        color: toggle.hovered ? Theme.surface : "transparent"
    }
    contentItem: RowLayout {
        spacing: Kirigami.Units.smallSpacing
        Glyph {
            name: toggle.open ? "go-down" : "go-next"
            tint: Theme.textMuted
        }
        QQC2.Label {
            text: toggle.text
            color: Theme.textMuted
        }
    }
}
