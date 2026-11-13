import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// The final check before anything runs: what it costs (password, restart,
// undo), then what gets installed, what gets changed and what is left alone,
// each with its reason. Every step, with its files, is under the details.
QQC2.ScrollView {
    id: page

    property var review: ({})
    readonly property var counts: review.counts || {}
    readonly property bool ready: review.ok === true && review.empty !== true
    readonly property int leftAlone: (review.provided || []).length + (review.skipped || []).length + (review.notes || []).length
    property alias detailsOpen: details.open

    contentWidth: availableWidth

    // Scroll so the details toggle sits at the top (the screenshot run).
    function showDetails() {
        Qt.callLater(() => page.contentItem.contentY = Math.max(0, details.y - Kirigami.Units.largeSpacing));
    }

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.mediumSpacing

        QQC2.Label {
            font.pointSize: Theme.headingSize
            font.weight: Font.DemiBold
            text: "Final review"
            color: Theme.text
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

        // What applying costs.
        GridLayout {
            Layout.fillWidth: true
            visible: page.ready
            columns: page.availableWidth > Kirigami.Units.gridUnit * 36 ? 3 : 1
            columnSpacing: Kirigami.Units.mediumSpacing
            rowSpacing: Kirigami.Units.smallSpacing
            Repeater {
                model: [
                    [page.review.password ? "Password once" : "No password", page.review.password
                        ? "Bindle asks when it starts, then runs every step." : "Nothing here needs admin rights.", page.review.password ? Theme.warning : Theme.success],
                    [page.review.needsReboot ? "Restart afterwards" : page.review.relogin ? "Log out afterwards" : "No restart needed",
                        page.review.needsReboot ? "For the new kernel, microcode or drivers." : page.review.relogin
                        ? "For the new group memberships." : "Everything takes effect right away.",
                        page.review.needsReboot || page.review.relogin ? Theme.warning : Theme.success],
                    ["Can be undone", "Changed files are saved first; \"bindle undo\" puts them back.", Theme.success]
                ]
                Rectangle {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    implicitHeight: cost.implicitHeight + 2 * Kirigami.Units.mediumSpacing
                    radius: Theme.radius
                    color: Theme.surfaceAlt
                    RowLayout {
                        id: cost
                        anchors.fill: parent
                        anchors.margins: Kirigami.Units.mediumSpacing
                        spacing: Kirigami.Units.mediumSpacing
                        Rectangle {
                            Layout.alignment: Qt.AlignTop
                            Layout.topMargin: 4
                            width: 10
                            height: 10
                            radius: 5
                            color: modelData[2]
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            QQC2.Label {
                                text: modelData[0]
                                color: Theme.text
                                font.weight: Font.DemiBold
                            }
                            QQC2.Label {
                                Layout.fillWidth: true
                                text: modelData[1]
                                wrapMode: Text.Wrap
                                color: Theme.textMuted
                                font.pointSize: Theme.smallSize
                            }
                        }
                    }
                }
            }
        }

        // Replacements need attention; what a pick pulled in is news.
        Repeater {
            model: (page.review.replaced || []).map(t => [t, "warning", "exchange-positions"])
                .concat((page.review.pulled_in || []).map(t => [t, "accent", "list-add"]))
            Notice {
                required property var modelData
                Layout.fillWidth: true
                text: modelData[0]
                tone: modelData[1]
                icon: modelData[2]
            }
        }

        GridLayout {
            Layout.fillWidth: true
            visible: page.ready
            columns: page.availableWidth > Kirigami.Units.gridUnit * 36 ? 3 : 1
            columnSpacing: Kirigami.Units.mediumSpacing
            rowSpacing: Kirigami.Units.mediumSpacing
            ReviewList {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                title: "Will install"
                count: (page.review.install || []).length + ((page.review.install || []).length === 1 ? " package" : " packages")
                rows: page.review.install || []
                mono: true
                empty: "No new packages."
            }
            ReviewList {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                title: "Will change"
                count: (page.counts.files === 1 ? "1 file, " : (page.counts.files || 0) + " files, ")
                    + (page.counts.services === 1 ? "1 service" : (page.counts.services || 0) + " services")
                rows: page.review.change || []
                mono: true
                empty: "No settings change."
            }
            ReviewList {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                title: "Left alone"
                count: page.leftAlone
                quiet: true
                rows: (page.review.provided || []).map(r => ({ what: r.what, why: "already set up: " + r.why }))
                    .concat((page.review.skipped || []).map(r => ({ what: r.what, why: "not here: " + r.why })))
                    .concat((page.review.notes || []).map(r => ({ what: r.what, why: r.why })))
                empty: "Nothing: every pick runs."
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
                        Pill {
                            text: step.modelData.root ? "as admin" : "as you"
                            tone: step.modelData.root ? "warning" : "muted"
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
