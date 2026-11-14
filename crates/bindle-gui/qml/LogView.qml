import QtQuick
import QtQuick.Controls as QQC2

// Command output in a panel, monospace, following the newest line.
Panel {
    id: logView

    property var lines: []

    color: Theme.surfaceAlt

    ListView {
        anchors.fill: parent
        clip: true
        model: logView.lines
        onCountChanged: positionViewAtEnd()
        delegate: QQC2.Label {
            required property string modelData
            width: ListView.view.width
            text: modelData
            wrapMode: Text.WrapAnywhere
            color: Theme.textMuted
            font.family: Theme.fontMono
            font.pointSize: Theme.smallSize
        }
    }
}
