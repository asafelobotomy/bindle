import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// What an update would do (crate::upgrade::Upgrade::check): what it costs
// (password, restart, snapshot), the Arch news since the last upgrade, then
// each stage with work and its switch. Stages with nothing to do sit in one
// quiet list at the end.
QQC2.ScrollView {
    id: page

    property var check: ({ state: "checking" })
    // Every stage's details open (the screenshot run).
    property bool openAll: false
    signal toggled(string id)
    signal openLink(string url)
    signal openBindle

    readonly property var stages: check.stages || []
    readonly property var busy: stages.filter(s => ["run", "report", "hold"].includes(s.kind) || s.switch)
    readonly property var calm: stages.filter(s => !busy.includes(s))

    contentWidth: availableWidth

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.mediumSpacing

        QQC2.Label {
            font.pointSize: Theme.headingSize
            font.weight: Font.DemiBold
            color: Theme.text
            text: page.check.state === "checking" ? "Checking for updates"
                : page.check.state === "error" ? "Couldn't check for updates"
                : (page.check.work || 0) > 0 ? "Updates are ready" : "Everything is up to date"
        }

        // Checking: what is being looked at, as it happens.
        RowLayout {
            visible: page.check.state === "checking"
            spacing: Kirigami.Units.mediumSpacing
            QQC2.BusyIndicator {
                running: parent.visible
                implicitWidth: Kirigami.Units.iconSizes.medium
                implicitHeight: Kirigami.Units.iconSizes.medium
            }
            QQC2.Label {
                text: "Nothing changes yet: this only looks."
                color: Theme.textMuted
            }
        }
        Repeater {
            model: page.check.state === "checking" ? (page.check.notes || []) : []
            QQC2.Label {
                required property string modelData
                required property int index
                text: (index + 1 < (page.check.notes || []).length ? "✓ " : "● ") + modelData
                color: index + 1 < (page.check.notes || []).length ? Theme.textMuted : Theme.text
            }
        }

        Notice {
            Layout.fillWidth: true
            visible: page.check.state === "error"
            tone: "error"
            icon: "dialog-error"
            text: page.check.error || ""
        }

        // What updating costs.
        GridLayout {
            Layout.fillWidth: true
            visible: page.check.state === "ready" && (page.check.work || 0) > 0
            columns: page.availableWidth > Kirigami.Units.gridUnit * 36 ? 3 : 1
            columnSpacing: Kirigami.Units.mediumSpacing
            rowSpacing: Kirigami.Units.smallSpacing
            Repeater {
                model: [
                    [page.check.password ? "Password once" : "No password", page.check.password
                        ? "Asked when the first system change starts." : "Nothing here needs admin rights.",
                        page.check.password ? Theme.warning : Theme.success],
                    page.check.restartBlocked
                        ? ["Fix before restarting", page.check.restartBlocked.replace(/^don't restart yet: /, ""), Theme.error]
                        : [page.check.reboot ? "Restart afterwards" : "No restart expected",
                            page.check.reboot ? page.check.reboot.replace(/^[a-z ]+: /, "") : "Nothing that needs one is updated.",
                            page.check.reboot ? Theme.warning : Theme.success],
                    [page.check.snapshot ? "Snapshot first" : "No snapshot", page.check.snapshot
                        ? "snapper saves the system before anything changes."
                        : page.check.snapper ? "No package upgrade runs, so none is needed." : "Set up snapper to roll an update back.",
                        page.check.snapshot ? Theme.success : Theme.textMuted]
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

        // Arch news since the last upgrade; a manual step comes first.
        Repeater {
            model: page.check.state === "ready" ? (page.check.news || []) : []
            Notice {
                required property var modelData
                Layout.fillWidth: true
                tone: modelData.manual ? "warning" : "accent"
                icon: modelData.manual ? "dialog-warning" : "news-subscribe"
                text: modelData.date + "  " + modelData.title + (modelData.manual ? " — read it before updating" : "")
                ActionButton {
                    text: "Read"
                    visible: modelData.link !== ""
                    onClicked: page.openLink(modelData.link)
                }
            }
        }

        // Stages with work, a note or a choice.
        Repeater {
            model: page.check.state === "ready" ? page.busy : []
            Panel {
                id: row
                required property var modelData
                readonly property bool off: !!modelData.switch && !modelData.on && modelData.kind === "run"
                Layout.fillWidth: true
                padding: Kirigami.Units.mediumSpacing
                edge: modelData.kind === "hold" ? Theme.warning : Theme.border
                implicitHeight: body.implicitHeight + 2 * padding
                ColumnLayout {
                    id: body
                    width: parent.width
                    spacing: Kirigami.Units.smallSpacing
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.mediumSpacing
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            RowLayout {
                                spacing: Kirigami.Units.smallSpacing
                                QQC2.Label {
                                    text: row.modelData.title
                                    color: row.off ? Theme.textMuted : Theme.text
                                    font.weight: Font.DemiBold
                                }
                                Pill {
                                    text: row.off ? "off" : row.modelData.kind === "run" ? "will run"
                                        : row.modelData.kind === "hold" ? "held" : "note"
                                    tone: row.off ? "muted" : row.modelData.kind === "run" ? "accent"
                                        : row.modelData.kind === "hold" ? "warning" : "muted"
                                }
                            }
                            QQC2.Label {
                                Layout.fillWidth: true
                                text: row.modelData.summary
                                wrapMode: Text.Wrap
                                color: Theme.textMuted
                            }
                            // Why it waits: not enough room, uQ's pin, battery, the news.
                            QQC2.Label {
                                Layout.fillWidth: true
                                visible: !!row.modelData.held
                                text: row.modelData.held || ""
                                wrapMode: Text.Wrap
                                color: Theme.warning
                            }
                        }
                        QQC2.Switch {
                            visible: !!row.modelData.switch
                            checked: row.modelData.on
                            text: row.modelData.switch || ""
                            onToggled: page.toggled(row.modelData.id)
                        }
                    }
                    Disclosure {
                        id: more
                        // The news is in the notices above.
                        visible: row.modelData.details.length > 0 && row.modelData.id !== "news"
                        open: page.openAll
                        text: row.modelData.details.length + (row.modelData.details.length === 1 ? " item" : " items")
                    }
                    Repeater {
                        model: more.open && more.visible ? row.modelData.details.slice(0, 40) : []
                        QQC2.Label {
                            required property string modelData
                            Layout.leftMargin: Kirigami.Units.gridUnit
                            Layout.fillWidth: true
                            text: modelData
                            wrapMode: Text.WrapAnywhere
                            color: Theme.textMuted
                            font.family: Theme.fontMono
                            font.pointSize: Theme.smallSize
                        }
                    }
                }
            }
        }

        Notice {
            Layout.fillWidth: true
            visible: page.check.state === "ready" && (page.check.newFeatures || 0) > 0
            tone: "accent"
            icon: "list-add"
            text: "What's new in Bindle: " + page.check.newFeatures + (page.check.newFeatures === 1 ? " item" : " items")
                + " to review (new features, changes, clean-ups). Nothing changes unless you pick it."
            ActionButton {
                text: "What's new"
                onClicked: page.openBindle()
            }
        }

        ReviewList {
            Layout.fillWidth: true
            visible: page.check.state === "ready" && page.calm.length > 0
            title: "Nothing to do"
            count: page.calm.length
            quiet: true
            rows: page.calm.map(s => ({ what: s.title, why: s.summary }))
            empty: ""
        }
    }
}
