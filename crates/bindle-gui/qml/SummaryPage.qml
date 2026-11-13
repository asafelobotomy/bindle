import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// The end of choosing, as an editable receipt: every selection by category,
// each removable in place, with a way back to change the category. Empty
// categories fold into one line. The footer's button goes on to the review.
QQC2.ScrollView {
    id: page

    property var categories: []
    property var selection: ({})
    // Modules the installer set up, which the plan leaves alone.
    property int alreadySetUp: 0
    signal edit(int index)
    signal remove(string id)

    readonly property var selected: selection.selected || ({})
    readonly property var needed: selection.needed || ({})
    readonly property int total: selection.total || 0
    readonly property int extra: Object.keys(needed).length
    // Per category: the modules picked, and those the plan adds for them.
    readonly property var cards: categories.map((c, index) => {
        const picked = [], added = [];
        for (const g of c.groups)
            for (const item of g.items)
                for (const m of item.modules) {
                    if (selected[m.id])
                        picked.push({ id: m.id, name: m.name });
                    else if (needed[m.id])
                        added.push({ name: m.name, by: needed[m.id] });
                }
        return { index: index, id: c.id, name: c.name, picked: picked, added: added };
    })
    readonly property var filled: cards.filter(c => c.picked.length + c.added.length > 0)
    readonly property var empty: cards.filter(c => c.picked.length + c.added.length === 0)

    contentWidth: availableWidth

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.mediumSpacing

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.mediumSpacing
            ColumnLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                QQC2.Label {
                    font.pointSize: Theme.headingSize
                    font.weight: Font.DemiBold
                    text: "Your selections"
                    color: Theme.text
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Theme.textMuted
                    text: page.total === 0 ? "Nothing is selected yet."
                        : "Remove anything with ×, or change a category. Nothing is applied until you confirm the final review."
                }
            }
            Repeater {
                model: [["Features", page.total], ["Added for them", page.extra], ["Already set up", page.alreadySetUp]]
                Panel {
                    required property var modelData
                    visible: modelData[1] > 0 || modelData[0] === "Features"
                    padding: Kirigami.Units.mediumSpacing
                    implicitWidth: Math.max(Kirigami.Units.gridUnit * 6, stat.implicitWidth + 2 * padding)
                    implicitHeight: stat.implicitHeight + 2 * padding
                    ColumnLayout {
                        id: stat
                        spacing: 0
                        QQC2.Label {
                            text: modelData[0]
                            color: Theme.textMuted
                            font.pointSize: Theme.smallSize
                        }
                        QQC2.Label {
                            text: modelData[1]
                            color: Theme.text
                            font.pointSize: Theme.headingSize
                            font.weight: Font.DemiBold
                        }
                    }
                }
            }
        }

        Repeater {
            model: page.filled
            ColumnLayout {
                id: sec
                required property var modelData
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: Theme.border
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.mediumSpacing
                    RowLayout {
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 10
                        Layout.maximumWidth: Kirigami.Units.gridUnit * 10
                        Layout.alignment: Qt.AlignTop
                        Layout.topMargin: Kirigami.Units.smallSpacing
                        spacing: Kirigami.Units.smallSpacing
                        Glyph {
                            name: Theme.categoryIcons[sec.modelData.id] || ""
                            tint: Theme.accent
                        }
                        QQC2.Label {
                            text: sec.modelData.name
                            color: Theme.text
                            font.weight: Font.DemiBold
                        }
                        QQC2.Label {
                            text: sec.modelData.picked.length
                            color: Theme.textMuted
                            font.pointSize: Theme.smallSize
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Repeater {
                            model: sec.modelData.picked
                            Rectangle {
                                id: chip
                                required property var modelData
                                implicitWidth: chipRow.implicitWidth + Kirigami.Units.mediumSpacing
                                implicitHeight: chipRow.implicitHeight + 2
                                radius: height / 2
                                color: Theme.surface
                                border.color: Theme.accent
                                Row {
                                    id: chipRow
                                    x: Kirigami.Units.mediumSpacing
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 2
                                    QQC2.Label {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: chip.modelData.name
                                        color: Theme.text
                                        font.pointSize: Theme.smallSize
                                    }
                                    QQC2.AbstractButton {
                                        id: rm
                                        implicitWidth: Kirigami.Units.gridUnit
                                        implicitHeight: implicitWidth
                                        hoverEnabled: true
                                        onClicked: page.remove(chip.modelData.id)
                                        Accessible.role: Accessible.Button
                                        Accessible.name: "Remove " + chip.modelData.name
                                        background: Rectangle {
                                            radius: width / 2
                                            color: rm.hovered ? Theme.overlay : "transparent"
                                        }
                                        contentItem: QQC2.Label {
                                            text: "×"
                                            horizontalAlignment: Text.AlignHCenter
                                            verticalAlignment: Text.AlignVCenter
                                            color: Theme.textMuted
                                        }
                                    }
                                }
                            }
                        }
                        Repeater {
                            model: sec.modelData.added
                            Pill {
                                required property var modelData
                                text: "+ " + modelData.name + " (for " + modelData.by + ")"
                            }
                        }
                    }
                    ActionButton {
                        Layout.alignment: Qt.AlignTop
                        text: "Change"
                        Accessible.name: "Change " + sec.modelData.name
                        onClicked: page.edit(sec.modelData.index)
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: Theme.border
            visible: page.empty.length > 0
        }
        Flow {
            Layout.fillWidth: true
            visible: page.empty.length > 0
            spacing: Kirigami.Units.smallSpacing
            QQC2.Label {
                // As tall as the buttons beside it (ActionButton's height).
                height: Kirigami.Units.gridUnit * 1.8
                verticalAlignment: Text.AlignVCenter
                text: "Nothing from"
                color: Theme.textMuted
            }
            Repeater {
                model: page.empty
                ActionButton {
                    required property var modelData
                    text: modelData.name
                    Accessible.name: "Add from " + modelData.name
                    onClicked: page.edit(modelData.index)
                }
            }
        }
    }
}
