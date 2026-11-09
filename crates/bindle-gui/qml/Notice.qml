import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A tinted band with an icon and one line of text: replacements, skips,
// restart notes. tone: accent, success, warning, error, muted. Children
// (a button) go at the end of the row.
Rectangle {
    id: notice

    property string text
    property string icon
    property string tone: "muted"
    default property alias actions: trailing.data
    readonly property color ink: tone === "accent" ? Theme.accent
        : tone === "success" ? Theme.success
        : tone === "warning" ? Theme.warning
        : tone === "error" ? Theme.error : Theme.textMuted

    radius: Theme.radius
    color: tone === "muted" ? Theme.surface : Qt.tint(Theme.surface, Qt.alpha(ink, 0.14))
    implicitHeight: row.implicitHeight + 2 * Kirigami.Units.smallSpacing

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.leftMargin: Kirigami.Units.mediumSpacing
        anchors.rightMargin: Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.mediumSpacing
        Glyph {
            name: notice.icon
            tint: notice.ink
        }
        QQC2.Label {
            Layout.fillWidth: true
            text: notice.text
            wrapMode: Text.Wrap
            color: notice.tone === "muted" ? Theme.textMuted : Theme.text
        }
        Row {
            id: trailing
        }
    }
}
