import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// What the installer set up: present::install_summary.
QQC2.ScrollView {
    id: page

    property var summary: ({})

    contentWidth: availableWidth

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            QQC2.Label {
                font.pointSize: Theme.headingSize
                font.weight: Font.DemiBold
                text: "Welcome to your new system"
                color: Theme.text
            }
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.textMuted
                text: [page.summary.installed, page.summary.source].filter(x => x).join(" · ")
            }
        }

        GridLayout {
            Layout.fillWidth: true
            columns: Math.max(1, Math.floor(page.availableWidth / (Kirigami.Units.gridUnit * 13)))
            columnSpacing: Kirigami.Units.mediumSpacing
            rowSpacing: Kirigami.Units.mediumSpacing

            Repeater {
                model: page.summary.rows || []
                Panel {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                    implicitHeight: cell.implicitHeight + 2 * padding
                    ColumnLayout {
                        id: cell
                        width: parent.width
                        spacing: 2
                        QQC2.Label {
                            text: modelData[0]
                            color: Theme.textMuted
                            font.pointSize: Theme.smallSize
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            text: modelData[1]
                            color: Theme.text
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }
                    }
                }
            }
        }

        QQC2.Label {
            visible: (page.summary.provided || []).length > 0
            text: "Already set up, so Bindle won't redo them"
            color: Theme.textMuted
        }
        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Repeater {
                model: page.summary.provided || []
                Pill {
                    required property var modelData
                    text: "✓ " + modelData[1]
                    tone: "success"
                }
            }
        }

        Repeater {
            model: page.summary.advice || []
            Panel {
                required property var modelData
                Layout.fillWidth: true
                edge: Theme.warning
                implicitHeight: tip.implicitHeight + 2 * padding
                QQC2.Label {
                    id: tip
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: modelData
                    color: Theme.text
                }
            }
        }
    }
}
