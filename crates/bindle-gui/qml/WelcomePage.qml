import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// What already works (the installer set it up) beside what Bindle can add,
// each suggestion a way into its category. The installer's full summary
// folds away under "System details".
QQC2.ScrollView {
    id: page

    // present::install_summary, plus `working`: [{name, how}].
    property var summary: ({})
    // "Arch Linux · intel · 15 GiB · nvidia · laptop".
    property string machine
    property var categories: []
    property var selection: ({})
    signal open(int index)

    readonly property var working: summary.working || []
    // Per category with suggestions: how many, and the first few names.
    readonly property var suggestions: {
        const picked = selection.selected || {};
        const out = [];
        categories.forEach((c, index) => {
            const names = [];
            for (const g of c.groups)
                for (const item of g.items)
                    for (const m of item.modules)
                        if (picked[m.id])
                            names.push(m.name);
            if (names.length > 0)
                out.push({ index: index, id: c.id, name: c.name, count: names.length,
                    names: names.slice(0, 3).join(", ") + (names.length > 3 ? ", and " + (names.length - 3) + " more" : "") });
        });
        return out;
    }

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
                text: page.machine + ". Here is what already works, and what Bindle suggests adding."
            }
        }

        GridLayout {
            Layout.fillWidth: true
            columns: page.availableWidth > Kirigami.Units.gridUnit * 34 ? 2 : 1
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: Kirigami.Units.largeSpacing

            Panel {
                Layout.fillWidth: true
                Layout.preferredWidth: 2
                Layout.alignment: Qt.AlignTop
                implicitHeight: workingBody.implicitHeight + 2 * padding
                ColumnLayout {
                    id: workingBody
                    width: parent.width
                    spacing: Kirigami.Units.smallSpacing
                    RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        QQC2.Label {
                            text: "Already working"
                            color: Theme.text
                            font.weight: Font.DemiBold
                        }
                        Pill {
                            text: page.working.length
                            tone: "success"
                        }
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: page.working.length > 0 ? "Set up by the installer. Bindle won't redo these."
                            : "Bindle couldn't tell what the installer set up, so it suggests everything that fits."
                        color: Theme.textMuted
                        font.pointSize: Theme.smallSize
                    }
                    Repeater {
                        model: page.working
                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.mediumSpacing
                            QQC2.Label {
                                Layout.alignment: Qt.AlignTop
                                text: "✓"
                                color: Theme.success
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    text: modelData.name
                                    color: Theme.text
                                    wrapMode: Text.Wrap
                                }
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    text: modelData.how
                                    color: Theme.textMuted
                                    font.pointSize: Theme.smallSize
                                    wrapMode: Text.Wrap
                                }
                            }
                        }
                    }
                }
            }

            Panel {
                Layout.fillWidth: true
                Layout.preferredWidth: 3
                Layout.alignment: Qt.AlignTop
                edge: Theme.accent
                implicitHeight: canBody.implicitHeight + 2 * padding
                ColumnLayout {
                    id: canBody
                    width: parent.width
                    spacing: Kirigami.Units.smallSpacing
                    RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        QQC2.Label {
                            text: "Bindle suggests"
                            color: Theme.text
                            font.weight: Font.DemiBold
                        }
                        Pill {
                            text: page.suggestions.reduce((n, s) => n + s.count, 0)
                            tone: "accent"
                        }
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: "Picked for this computer to start with. Open a category to see each one, or change it."
                        color: Theme.textMuted
                        font.pointSize: Theme.smallSize
                    }
                    Repeater {
                        model: page.suggestions
                        Rectangle {
                            id: sug
                            required property var modelData
                            Layout.fillWidth: true
                            implicitHeight: sugRow.implicitHeight + 2 * Kirigami.Units.smallSpacing
                            radius: Theme.radius
                            color: Theme.bg
                            RowLayout {
                                id: sugRow
                                anchors.fill: parent
                                anchors.leftMargin: Kirigami.Units.mediumSpacing
                                anchors.rightMargin: Kirigami.Units.smallSpacing
                                spacing: Kirigami.Units.mediumSpacing
                                Glyph {
                                    name: Theme.categoryIcons[sug.modelData.id] || ""
                                    tint: Theme.accent
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0
                                    QQC2.Label {
                                        text: sug.modelData.name + " · " + sug.modelData.count
                                        color: Theme.text
                                        font.weight: Font.Medium
                                    }
                                    QQC2.Label {
                                        Layout.fillWidth: true
                                        text: sug.modelData.names
                                        color: Theme.textMuted
                                        font.pointSize: Theme.smallSize
                                        elide: Text.ElideRight
                                    }
                                }
                                ActionButton {
                                    text: "Open"
                                    Accessible.name: "Open " + sug.modelData.name
                                    onClicked: page.open(sug.modelData.index)
                                }
                            }
                        }
                    }
                }
            }
        }

        Repeater {
            model: page.summary.advice || []
            Notice {
                required property string modelData
                Layout.fillWidth: true
                tone: "warning"
                icon: "dialog-warning"
                text: modelData
            }
        }

        Disclosure {
            id: details
            text: "System details: " + [page.summary.installed, page.summary.source].filter(x => x).join(" · ")
        }
        GridLayout {
            Layout.fillWidth: true
            visible: details.open
            columns: Math.max(1, Math.floor(page.availableWidth / (Kirigami.Units.gridUnit * 13)))
            columnSpacing: Kirigami.Units.mediumSpacing
            rowSpacing: Kirigami.Units.smallSpacing
            Repeater {
                model: details.open ? (page.summary.rows || []) : []
                ColumnLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                    spacing: 0
                    QQC2.Label {
                        text: modelData[0]
                        color: Theme.textMuted
                        font.pointSize: Theme.smallSize
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: modelData[1]
                        color: Theme.text
                        wrapMode: Text.Wrap
                    }
                }
            }
        }
    }
}
