import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// A small rounded label. tone: accent, success, warning, error, muted;
// filled draws it solid (the current step).
Rectangle {
    id: pill

    property string text
    property string tone: "muted"
    property bool filled: false
    readonly property color ink: tone === "accent" ? Theme.accent
        : tone === "success" ? Theme.success
        : tone === "warning" ? Theme.warning
        : tone === "error" ? Theme.error : Theme.textMuted

    implicitWidth: label.implicitWidth + 2 * Kirigami.Units.mediumSpacing
    implicitHeight: label.implicitHeight + Kirigami.Units.smallSpacing
    radius: height / 2
    color: filled ? Theme.accent : "transparent"
    border.color: filled ? Theme.accent : ink
    border.width: 1

    QQC2.Label {
        id: label
        anchors.centerIn: parent
        text: pill.text
        color: pill.filled ? Theme.accentText : pill.ink
        font.pointSize: Theme.smallSize
    }
}
