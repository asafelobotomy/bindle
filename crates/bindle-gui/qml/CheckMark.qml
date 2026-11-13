import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// The tick on a module: a box, or a circle in a "choose one" set (`radio`).
// mode: off, on, needed (the plan adds it for another module), provided
// (already set up) or na (not available on this computer).
Rectangle {
    id: mark

    property string mode: "off"
    property bool radio: false
    readonly property color ink: mode === "on" ? Theme.accent
        : mode === "provided" ? Theme.success
        : mode === "needed" ? Theme.accent : Theme.textMuted

    implicitWidth: Kirigami.Units.gridUnit * 1.1
    implicitHeight: implicitWidth
    radius: radio ? width / 2 : Math.min(4, Theme.radius)
    color: mode === "on" || mode === "provided" ? ink : "transparent"
    border.color: mode === "na" ? Theme.border : ink
    border.width: mode === "off" || mode === "na" ? 1 : 2
    // A short pop when the tick lands, so the click visibly did something.
    scale: 1
    onModeChanged: if (mode === "on") pop.restart()

    Behavior on color {
        ColorAnimation {
            duration: 120
        }
    }
    SequentialAnimation {
        id: pop
        NumberAnimation { target: mark; property: "scale"; to: 1.25; duration: 70 }
        NumberAnimation { target: mark; property: "scale"; to: 1; duration: 110; easing.type: Easing.OutBack }
    }
    QQC2.Label {
        anchors.centerIn: parent
        text: mark.mode === "needed" ? "+" : mark.mode === "na" ? "–" : "✓"
        visible: mark.mode !== "off"
        color: mark.mode === "on" || mark.mode === "provided" ? Theme.accentText
            : mark.mode === "needed" ? Theme.accent : Theme.textMuted
        font.pointSize: Theme.smallSize
        font.weight: Font.Bold
    }
}
