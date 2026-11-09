import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Pick a starting set of modules; the next screen fine-tunes it.
QQC2.ScrollView {
    id: page

    property var profiles: []
    property string current
    signal chosen(string id)

    contentWidth: availableWidth

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            QQC2.Label {
                font.pointSize: Theme.headingSize
                font.weight: Font.DemiBold
                text: "Choose a starting point"
                color: Theme.text
            }
            QQC2.Label {
                text: "You can change any module on the next screen."
                color: Theme.textMuted
            }
        }

        GridLayout {
            Layout.fillWidth: true
            columns: page.availableWidth > Kirigami.Units.gridUnit * 30 ? 2 : 1
            columnSpacing: Kirigami.Units.mediumSpacing
            rowSpacing: Kirigami.Units.mediumSpacing

            Repeater {
                model: page.profiles
                Panel {
                    id: card
                    required property var modelData
                    readonly property bool active: modelData.id === page.current

                    Layout.fillWidth: true
                    implicitHeight: body.implicitHeight + 2 * padding
                    edge: active || activeFocus ? Theme.accent : Theme.border
                    border.width: active || activeFocus ? 2 : 1

                    // Tab to a card, Space or Enter to choose it.
                    activeFocusOnTab: true
                    Keys.onSpacePressed: page.chosen(card.modelData.id)
                    Keys.onReturnPressed: page.chosen(card.modelData.id)
                    Keys.onEnterPressed: page.chosen(card.modelData.id)
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: card.modelData.name
                    Accessible.description: card.modelData.description
                    Accessible.checkable: true
                    Accessible.checked: card.active
                    Accessible.onPressAction: page.chosen(card.modelData.id)

                    ColumnLayout {
                        id: body
                        width: parent.width
                        spacing: Kirigami.Units.smallSpacing
                        RowLayout {
                            Layout.fillWidth: true
                            QQC2.Label {
                                Layout.fillWidth: true
                                text: card.modelData.name
                                color: Theme.text
                                font.weight: Font.DemiBold
                            }
                            Pill {
                                visible: card.active
                                text: "Selected"
                                tone: "accent"
                            }
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            text: card.modelData.description
                            color: Theme.textMuted
                            wrapMode: Text.Wrap
                        }
                        QQC2.Label {
                            text: card.modelData.count + " modules"
                            color: Theme.textMuted
                            font.pointSize: Theme.smallSize
                        }
                    }

                    TapHandler {
                        onTapped: page.chosen(card.modelData.id)
                    }
                }
            }
        }
    }
}
