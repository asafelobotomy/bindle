import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A window's header or footer: a surface strip with a hairline toward the
// page and a row for its children.
Rectangle {
    id: bar

    default property alias content: row.data
    property alias spacing: row.spacing
    // The hairline's side: the header's is at the bottom, the footer's on top.
    property bool footer: false

    implicitHeight: row.implicitHeight + 2 * Kirigami.Units.mediumSpacing
    color: Theme.surface

    Rectangle {
        y: bar.footer ? 0 : bar.height - height
        width: parent.width
        height: 1
        color: Theme.border
        // Over the row in the header, under it in the footer (as drawn first).
        z: bar.footer ? 0 : 1
    }

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.leftMargin: Kirigami.Units.largeSpacing
        anchors.rightMargin: Kirigami.Units.largeSpacing
    }
}
