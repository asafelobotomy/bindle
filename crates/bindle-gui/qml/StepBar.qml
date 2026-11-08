import QtQuick
import org.kde.kirigami as Kirigami

// Where the user is: one pill per screen, the current one filled.
Row {
    property var steps: []
    property int current: 0

    spacing: Kirigami.Units.smallSpacing

    Repeater {
        model: parent.steps
        Pill {
            required property string modelData
            required property int index
            text: modelData
            tone: index < parent.current ? "accent" : "muted"
            filled: index === parent.current
        }
    }
}
