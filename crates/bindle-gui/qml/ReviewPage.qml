import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// What applying will do, at a glance: one sentence, counts, the features by
// category and anything that needs attention. Every step, with the files
// it writes, sits under "Technical details".
QQC2.ScrollView {
    id: page

    property var review: ({})
    readonly property var counts: review.counts || {}
    property alias detailsOpen: details.open

    contentWidth: availableWidth

    // Scroll so the details toggle sits at the top (the screenshot run).
    function showDetails() {
        Qt.callLater(() => page.contentItem.contentY = Math.max(0, details.y - Kirigami.Units.largeSpacing));
    }

    // "Bindle will add 25 features. You'll be asked for your password once…"
    function summary() {
        const n = page.counts.modules || 0;
        let s = "Bindle will add " + n + (n === 1 ? " feature" : " features") + " to this computer.";
        if (page.review.password)
            s += " You'll be asked for your password once.";
        if (page.review.needsReboot)
            s += " Restart afterwards to finish.";
        else if (page.review.relogin)
            s += " Log out and back in afterwards.";
        return s;
    }

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.mediumSpacing

        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            QQC2.Label {
                font.pointSize: Theme.headingSize
                font.weight: Font.DemiBold
                text: "Review your changes"
                color: Theme.text
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: page.review.ok === true && page.review.empty !== true
                wrapMode: Text.Wrap
                text: page.summary()
                color: Theme.textMuted
            }
        }

        Notice {
            Layout.fillWidth: true
            visible: page.review.ok === false
            tone: "error"
            icon: "dialog-error"
            text: "Bindle can't build a plan from this selection: " + (page.review.error || "")
        }
        Notice {
            Layout.fillWidth: true
            visible: page.review.empty === true
            tone: "success"
            icon: "dialog-ok-apply"
            text: "Nothing to do: this system already matches the selection."
        }

        // Counts.
        RowLayout {
            Layout.fillWidth: true
            visible: page.review.ok === true && page.review.empty !== true
            spacing: Kirigami.Units.mediumSpacing
            Repeater {
                model: [["Features", page.counts.modules, "plugins"],
                    ["Packages", page.counts.packages, "package-x-generic"],
                    ["Settings files", page.counts.files, "document-edit"],
                    ["Services", page.counts.services, "system-run"]]
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
                        RowLayout {
                            Layout.alignment: Qt.AlignHCenter
                            spacing: Kirigami.Units.smallSpacing
                            Glyph {
                                name: modelData[2]
                                tint: Theme.textMuted
                            }
                            QQC2.Label {
                                text: modelData[0]
                                color: Theme.textMuted
                                font.pointSize: Theme.smallSize
                            }
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

        // What you get, by category.
        GridLayout {
            Layout.fillWidth: true
            columns: Math.max(1, Math.floor(page.availableWidth / (Kirigami.Units.gridUnit * 14)))
            columnSpacing: Kirigami.Units.mediumSpacing
            rowSpacing: Kirigami.Units.mediumSpacing
            Repeater {
                model: page.review.groups || []
                CategoryCard {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 13
                    Layout.alignment: Qt.AlignTop
                    categoryId: modelData.id
                    name: modelData.name
                    modules: modelData.modules
                }
            }
        }

        // Anything that needs attention: one band per line.
        Repeater {
            model: [[page.review.replaced, "warning", "exchange-positions"],
                [page.review.pulled_in, "accent", "list-add"],
                [page.review.provided, "success", "emblem-success"],
                [page.review.skipped, "muted", "list-remove"],
                [page.review.notes, "muted", "help-about"]]
            ColumnLayout {
                required property var modelData
                readonly property var kind: modelData
                Layout.fillWidth: true
                visible: (modelData[0] || []).length > 0
                spacing: 2
                Repeater {
                    model: kind[0] || []
                    Notice {
                        required property string modelData
                        Layout.fillWidth: true
                        text: modelData
                        tone: kind[1]
                        icon: kind[2]
                    }
                }
            }
        }

        Disclosure {
            id: details
            visible: (page.review.steps || []).length > 0
            text: "Technical details: " + (page.review.steps || []).length + " steps, with the files and commands"
        }
        Repeater {
            model: details.open ? (page.review.steps || []) : []
            Panel {
                id: step
                required property var modelData
                required property int index
                Layout.fillWidth: true
                padding: Kirigami.Units.smallSpacing
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
                            font.family: Theme.fontMono
                            font.pointSize: Theme.smallSize
                        }
                    }
                    Repeater {
                        model: step.modelData.content
                        QQC2.Label {
                            required property string modelData
                            Layout.leftMargin: Kirigami.Units.gridUnit + Kirigami.Units.mediumSpacing
                            Layout.fillWidth: true
                            text: modelData
                            // A long line (an fstab entry) wraps: every field stays readable.
                            wrapMode: Text.WrapAnywhere
                            color: Theme.textMuted
                            font.family: Theme.fontMono
                            font.pointSize: Theme.smallSize
                        }
                    }
                }
            }
        }
    }
}
