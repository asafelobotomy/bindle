import QtQuick
import org.kde.kirigami as Kirigami

// A bordered surface with padding; children go inside.
Rectangle {
    id: panel

    default property alias content: inner.data
    property int padding: Kirigami.Units.largeSpacing
    property color edge: Theme.border

    color: Theme.surface
    radius: Theme.radius
    border.color: edge
    border.width: 1
    implicitWidth: inner.childrenRect.width + 2 * padding
    implicitHeight: inner.childrenRect.height + 2 * padding

    Item {
        id: inner
        anchors.fill: parent
        anchors.margins: panel.padding
    }
}
