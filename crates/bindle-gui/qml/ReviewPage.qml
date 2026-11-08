import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// What applying will do: counts, replacements and skips, then every step.
QQC2.ScrollView {
    id: page

    property var review: ({})
    readonly property var counts: review.counts || {}

    contentWidth: availableWidth

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.mediumSpacing

        QQC2.Label {
            font.pointSize: Theme.headingSize
            font.weight: Font.DemiBold
            text: "Review your changes"
            color: Theme.text
        }

        Panel {
            Layout.fillWidth: true
            visible: page.review.ok === false
            edge: Theme.error
            implicitHeight: err.implicitHeight + 2 * padding
            QQC2.Label {
                id: err
                width: parent.width
                wrapMode: Text.Wrap
                text: "Bindle can't build a plan from this selection: " + (page.review.error || "")
                color: Theme.text
            }
        }

        QQC2.Label {
            visible: page.review.empty === true
            text: "Nothing to do: this system already matches the selection."
            color: Theme.success
        }

        RowLayout {
            Layout.fillWidth: true
            visible: page.review.ok === true
            spacing: Kirigami.Units.mediumSpacing
            Repeater {
                model: [["Modules", page.counts.modules], ["Packages", page.counts.packages],
                    ["Files", page.counts.files], ["Services", page.counts.services]]
                Rectangle {
                    required property var modelData
                    Layout.fillWidth: true
                    implicitHeight: tile.implicitHeight + 2 * Kirigami.Units.mediumSpacing
                    radius: Theme.radius
                    color: Theme.surface
                    ColumnLayout {
                        id: tile
                        anchors.centerIn: parent
                        spacing: 0
                        QQC2.Label {
                            Layout.alignment: Qt.AlignHCenter
                            text: modelData[0]
                            color: Theme.textMuted
                            font.pointSize: Theme.smallSize
                        }
                        QQC2.Label {
                            Layout.alignment: Qt.AlignHCenter
                            font.pointSize: Theme.headingSize
                            font.weight: Font.DemiBold
                            text: modelData[1] === undefined ? "0" : modelData[1]
                            color: Theme.text
                        }
                    }
                }
            }
        }

        Repeater {
            model: [[page.review.replaced, Theme.warning, "↔ "], [page.review.pulled_in, Theme.accent, "+ "],
                [page.review.provided, Theme.success, "✓ "], [page.review.skipped, Theme.textMuted, "– "]]
            ColumnLayout {
                required property var modelData
                Layout.fillWidth: true
                visible: (modelData[0] || []).length > 0
                spacing: 2
                Repeater {
                    model: modelData[0] || []
                    QQC2.Label {
                        required property string modelData
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: parent.modelData[2] + modelData
                        color: parent.modelData[1]
                    }
                }
            }
        }

        QQC2.Label {
            visible: (page.review.steps || []).length > 0
            Layout.topMargin: Kirigami.Units.mediumSpacing
            text: "Steps"
            color: Theme.text
            font.weight: Font.DemiBold
        }
        Repeater {
            model: page.review.steps || []
            Panel {
                id: step
                required property var modelData
                required property int index
                Layout.fillWidth: true
                padding: Kirigami.Units.mediumSpacing
                implicitHeight: stepBody.implicitHeight + 2 * padding
                ColumnLayout {
                    id: stepBody
                    width: parent.width
                    spacing: 2
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.mediumSpacing
                        QQC2.Label {
                            text: step.index + 1
                            color: Theme.textMuted
                            Layout.preferredWidth: Kirigami.Units.gridUnit
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            text: step.modelData.text
                            wrapMode: Text.Wrap
                            color: Theme.text
                        }
                        Pill {
                            visible: step.modelData.root
                            text: "admin"
                        }
                    }
                    Repeater {
                        model: step.modelData.content
                        QQC2.Label {
                            required property string modelData
                            Layout.leftMargin: Kirigami.Units.gridUnit + Kirigami.Units.mediumSpacing
                            text: modelData
                            color: Theme.textMuted
                            font.family: Theme.fontMono
                            font.pointSize: Theme.smallSize
                        }
                    }
                }
            }
        }

        QQC2.Label {
            visible: (page.review.notes || []).length > 0
            Layout.topMargin: Kirigami.Units.mediumSpacing
            text: "Notes"
            color: Theme.text
            font.weight: Font.DemiBold
        }
        Repeater {
            model: page.review.notes || []
            QQC2.Label {
                required property string modelData
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "• " + modelData
                color: Theme.textMuted
            }
        }
    }
}
