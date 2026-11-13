import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// Where the user is: one pill per screen, the current one filled, done ones
// ticked. A screen already reached is a button back to it.
Row {
    id: bar

    property var steps: []
    property int current: 0
    property int reached: 0
    // Shown after the current step's name ("2/10" on Choose).
    property string detail
    property bool locked: false
    signal picked(int index)

    spacing: Kirigami.Units.smallSpacing

    Repeater {
        model: bar.steps
        QQC2.AbstractButton {
            id: step
            required property string modelData
            required property int index
            readonly property bool now: index === bar.current
            readonly property bool done: index < bar.current || (index <= bar.reached && !now)
            enabled: !bar.locked && !now && index <= bar.reached
            hoverEnabled: true
            padding: Kirigami.Units.smallSpacing
            leftPadding: Kirigami.Units.mediumSpacing
            rightPadding: Kirigami.Units.mediumSpacing
            onClicked: bar.picked(index)
            Accessible.role: Accessible.Button
            Accessible.name: modelData + (now ? ", current step" : done ? ", done" : "")
            background: Rectangle {
                radius: height / 2
                color: step.now ? Theme.accent : step.hovered && step.enabled ? Theme.surfaceAlt : "transparent"
                border.color: step.now || step.done ? Theme.accent : Theme.border
                border.width: 1
            }
            contentItem: QQC2.Label {
                text: (step.done ? "✓ " : "") + step.modelData + (step.now && bar.detail !== "" ? " " + bar.detail : "")
                color: step.now ? Theme.accentText : step.done ? Theme.text : Theme.textMuted
                font.pointSize: Theme.smallSize
                font.weight: step.now ? Font.DemiBold : Font.Normal
            }
        }
    }
}
