import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// "What's new" after a Bindle upgrade (crate::model::whatsnew): what this
// Bindle would add, change or retire compared with what an earlier version
// set up, grouped like Choose's cards, each with a tick and its exact
// changes (file diffs). The picks then go through the usual Review.
QQC2.ScrollView {
    id: page

    property var whatsNew: ({ state: "none", items: [] })
    // Every item's changes open (the screenshot run).
    property bool openAll: false
    signal toggled(string id)
    signal changeToggled(string id, string key)
    signal hidden(string id)

    readonly property var items: whatsNew.items || []
    readonly property var groups: [
        ["new", "New for this computer", "Part of your setup, not on this computer yet."],
        ["updated", "Changed since it was set up", "This Bindle sets these up differently; the exact changes are below."],
        ["retired", "No longer in Bindle", "Tick to clean up what they left: their files and services. Packages stay."],
        ["removed", "You took these out of your setup", "What they set up is still here. Tick to clean it up: their files and services. Packages stay."],
        ["optional", "New in this version", "Optional: not part of your setup unless you tick them."]
    ]

    contentWidth: availableWidth

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.mediumSpacing

        QQC2.Label {
            font.pointSize: Theme.headingSize
            font.weight: Font.DemiBold
            color: Theme.text
            text: "What's new in Bindle " + (page.whatsNew.version || "")
        }
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.textMuted
            text: (page.whatsNew.since ? "Since Bindle " + page.whatsNew.since + " set this computer up. " : "")
                + "Tick what to apply; the next page shows every change before anything runs, and Bindle saves the files first so it can be undone."
        }
        Notice {
            Layout.fillWidth: true
            visible: page.whatsNew.recorded === false
            tone: "warning"
            icon: "dialog-information"
            text: "An older Bindle set this computer up without keeping a record, so this compares with what is installed."
        }

        Repeater {
            model: page.groups
            ColumnLayout {
                id: group
                required property var modelData
                readonly property var members: page.items.filter(i => i.kind === modelData[0])
                Layout.fillWidth: true
                visible: members.length > 0
                spacing: Kirigami.Units.smallSpacing

                RowLayout {
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    spacing: Kirigami.Units.smallSpacing
                    QQC2.Label {
                        text: group.modelData[1]
                        color: Theme.text
                        font.weight: Font.DemiBold
                    }
                    Pill {
                        text: group.members.length
                    }
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: group.modelData[2]
                    wrapMode: Text.Wrap
                    color: Theme.textMuted
                    font.pointSize: Theme.smallSize
                }

                Repeater {
                    model: group.members
                    Panel {
                        id: card
                        required property var modelData
                        Layout.fillWidth: true
                        padding: Kirigami.Units.mediumSpacing
                        edge: modelData.picked ? Theme.accent : Theme.border
                        implicitHeight: body.implicitHeight + 2 * padding
                        ColumnLayout {
                            id: body
                            width: parent.width
                            spacing: Kirigami.Units.smallSpacing
                            QQC2.AbstractButton {
                                id: tick
                                Layout.fillWidth: true
                                hoverEnabled: true
                                onClicked: page.toggled(card.modelData.id)
                                Accessible.role: Accessible.CheckBox
                                Accessible.name: card.modelData.name
                                Accessible.checked: card.modelData.picked
                                contentItem: RowLayout {
                                    spacing: Kirigami.Units.mediumSpacing
                                    CheckMark {
                                        Layout.alignment: Qt.AlignTop
                                        mode: card.modelData.picked ? "on" : "off"
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 0
                                        QQC2.Label {
                                            text: card.modelData.name
                                            color: Theme.text
                                            font.weight: Font.DemiBold
                                        }
                                        QQC2.Label {
                                            Layout.fillWidth: true
                                            text: card.modelData.why
                                            wrapMode: Text.Wrap
                                            color: Theme.textMuted
                                        }
                                    }
                                }
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                Disclosure {
                                    id: more
                                    visible: card.modelData.changes.length > 0
                                    open: page.openAll
                                    text: card.modelData.changes.length + (card.modelData.changes.length === 1 ? " change" : " changes")
                                }
                                Item {
                                    Layout.fillWidth: true
                                }
                                QQC2.AbstractButton {
                                    id: hide
                                    hoverEnabled: true
                                    padding: 2
                                    onClicked: page.hidden(card.modelData.id)
                                    Accessible.role: Accessible.Button
                                    Accessible.name: "Hide " + card.modelData.name
                                    QQC2.ToolTip.visible: hovered
                                    QQC2.ToolTip.text: "Don't show this again (unless a later Bindle changes it again)"
                                    contentItem: QQC2.Label {
                                        text: "Hide"
                                        color: Theme.textMuted
                                        font.underline: hide.hovered
                                        font.pointSize: Theme.smallSize
                                    }
                                }
                            }
                            Repeater {
                                model: more.open && more.visible ? card.modelData.changes : []
                                ColumnLayout {
                                    id: change
                                    required property var modelData
                                    Layout.fillWidth: true
                                    Layout.leftMargin: Kirigami.Units.gridUnit
                                    spacing: 0
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: Kirigami.Units.smallSpacing
                                        // A change that can be left out on its own has its own tick.
                                        QQC2.AbstractButton {
                                            visible: !!change.modelData.key
                                            enabled: card.modelData.picked
                                            onClicked: page.changeToggled(card.modelData.id, change.modelData.key)
                                            Accessible.role: Accessible.CheckBox
                                            Accessible.name: change.modelData.text
                                            Accessible.checked: change.modelData.picked
                                            contentItem: CheckMark {
                                                implicitWidth: Kirigami.Units.gridUnit * 0.8
                                                mode: !card.modelData.picked ? "na" : change.modelData.picked ? "on" : "off"
                                            }
                                        }
                                        QQC2.Label {
                                            Layout.fillWidth: true
                                            text: change.modelData.text
                                            wrapMode: Text.Wrap
                                            color: change.modelData.picked || !change.modelData.key ? Theme.text : Theme.textMuted
                                            font.strikeout: !!change.modelData.key && !change.modelData.picked
                                            font.family: Theme.fontMono
                                            font.pointSize: Theme.smallSize
                                        }
                                    }
                                    Repeater {
                                        model: change.modelData.diff || []
                                        QQC2.Label {
                                            required property string modelData
                                            Layout.fillWidth: true
                                            Layout.leftMargin: Kirigami.Units.gridUnit
                                            text: modelData
                                            wrapMode: Text.WrapAnywhere
                                            color: modelData.startsWith("+") ? Theme.success
                                                : modelData.startsWith("-") ? Theme.error : Theme.textMuted
                                            font.family: Theme.fontMono
                                            font.pointSize: Theme.smallSize
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
