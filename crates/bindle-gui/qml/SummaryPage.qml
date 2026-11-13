import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// The end of choosing: everything selected, by category, each with a way
// back to change it, and the step on to the final review.
QQC2.ScrollView {
    id: page

    property var categories: []
    property var selection: ({})
    signal edit(int index)
    signal confirmed

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
                        picked.push(m.name);
                    else if (needed[m.id])
                        added.push({ name: m.name, by: needed[m.id] });
                }
        return { index: index, id: c.id, name: c.name, picked: picked, added: added };
    })
    readonly property int filled: cards.filter(c => c.picked.length + c.added.length > 0).length

    contentWidth: availableWidth

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
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
                    : page.total + (page.total === 1 ? " feature" : " features") + " in " + page.filled
                        + (page.filled === 1 ? " category" : " categories")
                        + (page.extra > 0 ? ", plus " + page.extra + " that they need" : "")
                        + ". Nothing changes on this computer until you confirm the final review."
            }
        }

        GridLayout {
            Layout.fillWidth: true
            columns: Math.max(1, Math.floor(page.availableWidth / (Kirigami.Units.gridUnit * 20)))
            columnSpacing: Kirigami.Units.mediumSpacing
            rowSpacing: Kirigami.Units.mediumSpacing

            Repeater {
                model: page.cards
                Panel {
                    id: card
                    required property var modelData
                    readonly property bool empty: modelData.picked.length + modelData.added.length === 0

                    Layout.fillWidth: true
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 19
                    Layout.alignment: Qt.AlignTop
                    padding: Kirigami.Units.mediumSpacing
                    implicitHeight: body.implicitHeight + 2 * padding
                    opacity: empty ? 0.75 : 1

                    ColumnLayout {
                        id: body
                        width: parent.width
                        spacing: Kirigami.Units.smallSpacing

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Glyph {
                                name: Theme.categoryIcons[card.modelData.id] || ""
                                tint: card.empty ? Theme.textMuted : Theme.accent
                            }
                            QQC2.Label {
                                text: card.modelData.name
                                color: Theme.text
                                font.weight: Font.DemiBold
                            }
                            QQC2.Label {
                                Layout.fillWidth: true
                                text: card.empty ? "nothing selected" : card.modelData.picked.length + " selected"
                                color: Theme.textMuted
                                font.pointSize: Theme.smallSize
                            }
                            ActionButton {
                                text: card.empty ? "Add" : "Change"
                                Accessible.name: text + " " + card.modelData.name
                                onClicked: page.edit(card.modelData.index)
                            }
                        }
                        Flow {
                            Layout.fillWidth: true
                            visible: !card.empty
                            spacing: Kirigami.Units.smallSpacing
                            Repeater {
                                model: card.modelData.picked
                                Pill {
                                    required property string modelData
                                    text: modelData
                                    tone: "accent"
                                }
                            }
                            Repeater {
                                model: card.modelData.added
                                Pill {
                                    required property var modelData
                                    text: "+ " + modelData.name + " (for " + modelData.by + ")"
                                }
                            }
                        }
                    }
                }
            }
        }

        Notice {
            Layout.fillWidth: true
            tone: page.total === 0 ? "muted" : "accent"
            icon: "dialog-ok-apply"
            text: page.total === 0 ? "Go back and select something, or continue to see what is already set up."
                : "Happy with these? The final review lists every package, file and service before anything is applied."
            ActionButton {
                primary: true
                text: "Go to final review"
                onClicked: page.confirmed()
            }
        }
    }
}
