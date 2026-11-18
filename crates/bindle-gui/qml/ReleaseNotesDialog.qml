import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// "What's new in Bindle X.Y.Z": the release's own highlights (from the
// CHANGELOG), shown once after Bindle was updated. "Open What's new" goes to
// the per-item review (`canOpen`: something waits in it); "Dismiss" and
// Escape close it. Either answer is final for this version.
QQC2.Popup {
    id: dialog

    // `bindle_run::relnotes::ReleaseNotes` as JSON, or {} for none.
    property var notes: ({})
    property bool canOpen: false

    signal openWhatsNew
    signal dismissed

    readonly property var highlights: notes.highlights || []

    parent: QQC2.Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: Kirigami.Units.largeSpacing * 1.5
    width: Math.min(Kirigami.Units.gridUnit * 34, parent ? parent.width - Kirigami.Units.gridUnit * 2 : 0)
    closePolicy: QQC2.Popup.CloseOnEscape
    onClosed: dialog.dismissed()

    background: Rectangle {
        radius: Theme.radius
        color: Theme.surface
        border.color: Theme.border
        border.width: 1
    }
    QQC2.Overlay.modal: Rectangle {
        color: Qt.alpha("black", 0.35)
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.mediumSpacing

        QQC2.Label {
            Layout.fillWidth: true
            text: dialog.notes.title || ""
            font.pointSize: Theme.headingSize
            font.weight: Font.DemiBold
            color: Theme.text
            wrapMode: Text.Wrap
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: !!dialog.notes.date
            text: "Released " + (dialog.notes.date || "")
            color: Theme.textMuted
        }
        Repeater {
            model: dialog.highlights

            ColumnLayout {
                required property var modelData

                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.smallSpacing
                spacing: 2

                QQC2.Label {
                    Layout.fillWidth: true
                    text: modelData.title
                    font.weight: Font.DemiBold
                    color: Theme.text
                    wrapMode: Text.Wrap
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    visible: modelData.detail !== ""
                    text: modelData.detail
                    color: Theme.textMuted
                    wrapMode: Text.Wrap
                }
            }
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: (dialog.notes.more || 0) > 0
            text: "and " + dialog.notes.more + " more in the changelog"
            color: Theme.textMuted
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.mediumSpacing

            Item {
                Layout.fillWidth: true
            }
            ActionButton {
                text: "Dismiss"
                onClicked: dialog.close()
            }
            ActionButton {
                visible: dialog.canOpen
                primary: true
                text: "Open What's new"
                onClicked: {
                    dialog.openWhatsNew();
                    dialog.close();
                }
            }
        }
    }
}
