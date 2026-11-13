import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One group of a category ("Browsers"): a heading, then its modules in the
// current view. Modules that can't be combined sit together in a framed
// "Choose one" box, drawn with radio marks. The heading can select or clear
// the whole group; a card's details open under the group.
Column {
    id: section

    property string title
    property var items: []
    property string view: "grid"
    property var selected: ({})
    property var needed: ({})
    property string currentId
    signal toggled(string id)
    signal inspected(string id)
    signal setAll(var ids, bool on)
    // Details opened under the group: the page scrolls them into view.
    signal detailsShown(var item)

    readonly property int gap: view === "compact" ? 2 : Kirigami.Units.mediumSpacing
    readonly property real cardWidth: Kirigami.Units.gridUnit * 13
    readonly property int picked: {
        let n = 0;
        for (const item of items)
            for (const m of item.modules)
                if (selected[m.id])
                    n += 1;
        return n;
    }
    readonly property int count: items.reduce((n, item) => n + item.modules.length, 0)
    // Modules "Select all" acts on: not in a "choose one" set, and tickable.
    readonly property var togglable: {
        const out = [];
        for (const item of items)
            if (!item.choice)
                for (const m of item.modules)
                    if (!m.provided && !m.unavailable)
                        out.push(m.id);
        return out;
    }
    readonly property bool allOn: togglable.length > 0 && togglable.every(id => selected[id] === true)
    // The module whose details are open, when it is in this group.
    readonly property var current: {
        for (const item of items)
            for (const m of item.modules)
                if (m.id === currentId)
                    return m;
        return null;
    }

    // Cards across `width`, and the width each gets.
    function cell(width) {
        if (view !== "grid")
            return width;
        const columns = Math.max(1, Math.floor((width + gap) / (cardWidth + gap)));
        return Math.floor((width - (columns - 1) * gap) / columns);
    }

    spacing: Kirigami.Units.smallSpacing

    RowLayout {
        width: parent.width
        visible: section.title !== ""
        spacing: Kirigami.Units.smallSpacing
        QQC2.Label {
            text: section.title
            color: Theme.text
            font.weight: Font.DemiBold
        }
        QQC2.Label {
            text: section.picked > 0 ? section.picked + " of " + section.count + " selected" : section.count
            color: section.picked > 0 ? Theme.accent : Theme.textMuted
            font.pointSize: Theme.smallSize
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            height: 1
            color: Theme.border
        }
        QQC2.AbstractButton {
            id: all
            visible: section.togglable.length > 1
            hoverEnabled: true
            padding: Kirigami.Units.smallSpacing
            onClicked: section.setAll(section.togglable, !section.allOn)
            Accessible.role: Accessible.Button
            Accessible.name: text + " in " + section.title
            text: section.allOn ? "Select none" : "Select all"
            contentItem: QQC2.Label {
                text: all.text
                color: Theme.accent
                font.pointSize: Theme.smallSize
                font.underline: all.hovered
            }
        }
    }

    Flow {
        width: parent.width
        spacing: section.gap

        Repeater {
            model: section.items
            Rectangle {
                id: slot
                required property var modelData
                readonly property bool choice: modelData.choice
                readonly property int pad: choice ? Kirigami.Units.mediumSpacing : 0

                width: choice ? section.width : section.cell(section.width)
                height: inner.implicitHeight + 2 * pad
                radius: Theme.radius
                color: choice ? Qt.tint(Theme.bg, Qt.alpha(Theme.warning, 0.07)) : "transparent"
                border.color: choice ? Qt.alpha(Theme.warning, 0.5) : "transparent"
                border.width: choice ? 1 : 0

                Column {
                    id: inner
                    x: slot.pad
                    y: slot.pad
                    width: slot.width - 2 * slot.pad
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        visible: slot.choice
                        width: parent.width
                        spacing: Kirigami.Units.smallSpacing
                        Pill {
                            text: "Choose one"
                            tone: "warning"
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            text: "These do the same job and can't be installed together. Picking one swaps out the other."
                            color: Theme.textMuted
                            font.pointSize: Theme.smallSize
                            elide: Text.ElideRight
                        }
                    }
                    Flow {
                        width: parent.width
                        spacing: section.gap
                        Repeater {
                            model: slot.modelData.modules
                            ModuleCard {
                                required property var modelData
                                width: section.cell(inner.width)
                                module: modelData
                                view: section.view
                                radio: slot.choice
                                selected: section.selected[modelData.id] === true
                                neededBy: section.needed[modelData.id] || ""
                                current: section.currentId === modelData.id
                                onPicked: section.toggled(modelData.id)
                                onInspected: section.inspected(modelData.id)
                            }
                        }
                    }
                }
            }
        }
    }

    ModuleDetails {
        id: details
        width: parent.width
        onModuleChanged: if (module !== null) section.detailsShown(details)
        visible: section.current !== null
        module: section.current
        selected: section.current !== null && section.selected[section.current.id] === true
        neededBy: section.current !== null ? (section.needed[section.current.id] || "") : ""
        onClosed: section.inspected(section.currentId)
    }
}
