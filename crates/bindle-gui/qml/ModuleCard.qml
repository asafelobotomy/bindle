import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One module, in any of the three views: a card (`grid`), a row with its
// description (`list`) or a one-line row (`compact`). The whole surface
// toggles it; the tick, the border and the tint all show the result.
QQC2.AbstractButton {
    id: card

    property var module: ({})
    property string view: "grid"
    property bool radio: false
    property bool selected: false
    // "Steam": the plan adds this module because Steam needs it.
    property string neededBy
    property bool current: false
    signal toggled
    signal inspected

    readonly property bool provided: !!module.provided
    readonly property bool unavailable: !provided && !!module.unavailable
    readonly property bool locked: provided || unavailable
    readonly property string mode: provided ? "provided" : unavailable ? "na"
        : selected ? "on" : neededBy !== "" ? "needed" : "off"
    readonly property bool grid: view === "grid"
    readonly property bool compact: view === "compact"
    // The one state worth a label, and its tone.
    readonly property var status: provided ? ["Already set up", "success"]
        : unavailable ? ["Not available here", "muted"]
        : neededBy !== "" && !selected ? ["Needed by " + neededBy, "accent"] : null

    hoverEnabled: true
    padding: compact ? Kirigami.Units.smallSpacing : Kirigami.Units.mediumSpacing
    leftPadding: Kirigami.Units.mediumSpacing
    rightPadding: Kirigami.Units.mediumSpacing
    opacity: unavailable ? 0.6 : 1
    onHoveredChanged: if (hovered) inspected()
    onActiveFocusChanged: if (activeFocus) inspected()
    onClicked: {
        inspected();
        if (!locked)
            toggled();
    }
    Accessible.role: radio ? Accessible.RadioButton : Accessible.CheckBox
    Accessible.name: module.name || ""
    Accessible.description: (status ? status[0] + ". " : "") + (module.description || "")
    Accessible.checkable: !locked
    Accessible.checked: selected || provided

    background: Rectangle {
        radius: Theme.radius
        color: card.selected ? Qt.tint(Theme.surface, Qt.alpha(Theme.accent, 0.16))
            : card.hovered || card.current ? Theme.surfaceAlt
            : card.compact ? "transparent" : Theme.surface
        border.color: card.selected ? Theme.accent
            : card.activeFocus ? Theme.textMuted
            : card.compact ? "transparent" : Theme.border
        border.width: card.selected ? 2 : 1
        Behavior on color {
            ColorAnimation {
                duration: 120
            }
        }
    }

    contentItem: GridLayout {
        // Grid: tick and name, then the description, then labels.
        // Rows: tick, name, description (list only), labels, all on one line.
        columns: card.grid ? 2 : 4
        columnSpacing: Kirigami.Units.mediumSpacing
        rowSpacing: Kirigami.Units.smallSpacing

        CheckMark {
            mode: card.mode
            radio: card.radio
            Layout.alignment: Qt.AlignVCenter
        }
        QQC2.Label {
            Layout.fillWidth: card.grid || card.compact
            Layout.preferredWidth: card.grid || card.compact ? -1 : Kirigami.Units.gridUnit * 11
            text: card.module.name || ""
            elide: Text.ElideRight
            color: Theme.text
            font.weight: card.selected || card.grid ? Font.DemiBold : Font.Normal
        }
        QQC2.Label {
            id: blurb
            visible: !card.compact
            Layout.columnSpan: card.grid ? 2 : 1
            Layout.fillWidth: true
            // Two lines in a card, always: every card is the same height.
            Layout.preferredHeight: card.grid ? Math.ceil(lines.lineSpacing * 2) + 2 : -1
            text: card.module.description || ""
            wrapMode: card.grid ? Text.Wrap : Text.NoWrap
            maximumLineCount: card.grid ? 2 : 1
            elide: Text.ElideRight
            verticalAlignment: Text.AlignTop
            color: Theme.textMuted
            font.pointSize: Theme.smallSize
            FontMetrics {
                id: lines
                font: blurb.font
            }
        }
        RowLayout {
            Layout.columnSpan: card.grid ? 2 : 1
            Layout.fillWidth: card.grid
            Layout.preferredHeight: card.grid ? sizer.implicitHeight : -1
            spacing: Kirigami.Units.smallSpacing
            Pill {
                id: sizer
                visible: false
                text: "X"
            }
            Pill {
                visible: card.status !== null
                text: card.status ? card.status[0] : ""
                tone: card.status ? card.status[1] : "muted"
            }
            Pill {
                visible: !!card.module.risk && !card.locked
                text: card.module.risk === "advanced" ? "Advanced" : "Moderate"
                tone: card.module.risk === "advanced" ? "error" : "warning"
            }
            Pill {
                visible: !!card.module.source && !card.locked && !card.compact
                text: card.module.source || ""
            }
            Item {
                visible: card.grid
                Layout.fillWidth: true
            }
        }
    }
}
