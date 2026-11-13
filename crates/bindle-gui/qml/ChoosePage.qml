import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One category at a time: its groups of modules as cards or rows, with
// search, filters and a view switch above, the hovered module's details
// beside, and a line under the list saying what the last click changed.
RowLayout {
    id: page

    // Backend.catalogJson: fixed for the run, so a tick never rebuilds the list.
    property var categories: []
    // Backend.stateJson: what is ticked now.
    property var selection: ({})
    property var feedback: ({})
    property int categoryIndex: 0
    property string moduleId
    // grid, list or compact; kept while moving between categories.
    property string view: "grid"
    // all, selected or suggested.
    property string filter: "all"
    property bool hideUnavailable: false
    property bool safeOnly: false
    signal toggled(string id)

    readonly property var selected: selection.selected || ({})
    readonly property var needed: selection.needed || ({})
    readonly property var category: categories[categoryIndex] || ({ groups: [] })
    readonly property bool last: categoryIndex >= categories.length - 1
    readonly property string nextName: last ? "" : categories[categoryIndex + 1].name
    readonly property string query: search.text.trim().toLowerCase()
    // The ticks as they were when the filter was chosen: unticking under
    // "Selected" leaves the card in place instead of pulling it away.
    property var frozen: ({ selected: {}, suggested: {} })

    function freeze() {
        frozen = { selected: Object.assign({}, selection.selected), suggested: selection.suggested || {} };
    }
    function passes(m, status) {
        if (status === "selected" && !frozen.selected[m.id])
            return false;
        if (status === "suggested" && !frozen.suggested[m.id])
            return false;
        if (hideUnavailable && !m.provided && m.unavailable)
            return false;
        if (safeOnly && m.risk !== "")
            return false;
        return query === "" || (m.name + " " + m.id + " " + m.description).toLowerCase().includes(query);
    }
    // Groups to draw: this category's, or every category's while searching.
    function sectionsFor(status) {
        const out = [];
        for (const c of (query === "" ? [category] : categories)) {
            for (const g of c.groups) {
                const items = [];
                for (const item of g.items) {
                    const modules = item.modules.filter(m => passes(m, status));
                    if (modules.length > 0)
                        items.push({ choice: item.choice, modules: modules });
                }
                if (items.length > 0)
                    out.push({ title: query === "" ? g.name : c.name + " · " + g.name, items: items });
            }
        }
        return out;
    }
    function size(sections) {
        return sections.reduce((n, s) => n + s.items.reduce((k, i) => k + i.modules.length, 0), 0);
    }
    readonly property var sections: sectionsFor(filter)
    readonly property var current: {
        for (const c of categories)
            for (const g of c.groups)
                for (const item of g.items)
                    for (const m of item.modules)
                        if (m.id === moduleId)
                            return m;
        return null;
    }

    function open(index) {
        search.text = "";
        categoryIndex = Math.max(0, Math.min(categories.length - 1, index));
        moduleId = "";
        freeze();
        toast.stop();
        list.contentItem.contentY = 0;
    }
    function setFilter(name) {
        freeze();
        filter = name;
        list.contentItem.contentY = 0;
    }

    onVisibleChanged: if (visible) freeze()
    onFeedbackChanged: if (feedback.text) toast.restart()
    spacing: Kirigami.Units.largeSpacing

    // Categories: where you are, and how much each holds.
    ListView {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 11
        Layout.fillHeight: true
        clip: true
        model: page.categories
        spacing: 2
        delegate: QQC2.ItemDelegate {
            id: cat
            required property var modelData
            required property int index
            readonly property bool active: index === page.categoryIndex && page.query === ""
            readonly property int picked: (page.selection.counts || {})[modelData.id] || 0
            width: ListView.view.width
            onClicked: page.open(index)
            Accessible.name: modelData.name + ", " + picked + " of " + modelData.total + " selected"
            background: Rectangle {
                radius: Theme.radius
                color: cat.active ? Theme.accent : cat.hovered ? Theme.surfaceAlt : "transparent"
            }
            contentItem: RowLayout {
                spacing: Kirigami.Units.smallSpacing
                Glyph {
                    name: Theme.categoryIcons[cat.modelData.id] || ""
                    tint: cat.active ? Theme.accentText : Theme.textMuted
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: cat.modelData.name
                    elide: Text.ElideRight
                    color: cat.active ? Theme.accentText : Theme.text
                }
                Rectangle {
                    visible: cat.picked > 0
                    implicitWidth: Math.max(height, tally.implicitWidth + Kirigami.Units.mediumSpacing)
                    implicitHeight: tally.implicitHeight + 2
                    radius: height / 2
                    color: cat.active ? Theme.accentText : Qt.tint(Theme.surface, Qt.alpha(Theme.accent, 0.25))
                    QQC2.Label {
                        id: tally
                        anchors.centerIn: parent
                        text: cat.picked
                        color: Theme.accent
                        font.pointSize: Theme.smallSize
                        font.weight: Font.DemiBold
                    }
                }
            }
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.mediumSpacing
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                QQC2.Label {
                    Layout.fillWidth: true
                    text: page.query === "" ? (page.category.name || "") : "Search results"
                    font.pointSize: Theme.headingSize
                    font.weight: Font.DemiBold
                    color: Theme.text
                    elide: Text.ElideRight
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: page.query === "" ? (page.category.description || "")
                        : page.size(page.sections) + " matches in every category"
                    color: Theme.textMuted
                    elide: Text.ElideRight
                }
            }
            QQC2.Label {
                visible: page.query === ""
                text: "Step " + (page.categoryIndex + 1) + " of " + page.categories.length
                color: Theme.textMuted
                font.pointSize: Theme.smallSize
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            QQC2.TextField {
                id: search
                Layout.fillWidth: true
                placeholderText: "Search every category"
                onTextChanged: list.contentItem.contentY = 0
            }
            Repeater {
                model: [["grid", "Grid"], ["list", "List"], ["compact", "Compact"]]
                Chip {
                    required property var modelData
                    text: modelData[1]
                    active: page.view === modelData[0]
                    onClicked: page.view = modelData[0]
                }
            }
        }

        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Repeater {
                model: [["all", "All"], ["selected", "Selected"], ["suggested", "Suggested"]]
                Chip {
                    required property var modelData
                    text: modelData[1]
                    count: page.size(page.sectionsFor(modelData[0]))
                    active: page.filter === modelData[0]
                    onClicked: page.setFilter(modelData[0])
                }
            }
            Rectangle {
                width: 1
                height: parent.height
                color: Theme.border
            }
            Chip {
                text: "Hide unavailable"
                active: page.hideUnavailable
                onClicked: page.hideUnavailable = !page.hideUnavailable
            }
            Chip {
                text: "Safe only"
                active: page.safeOnly
                onClicked: page.safeOnly = !page.safeOnly
            }
        }

        QQC2.ScrollView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true

            Column {
                width: list.availableWidth
                spacing: Kirigami.Units.largeSpacing
                bottomPadding: Kirigami.Units.largeSpacing

                Repeater {
                    model: page.sections
                    ModuleSection {
                        required property var modelData
                        width: parent.width
                        title: modelData.title
                        items: modelData.items
                        view: page.view
                        selected: page.selected
                        needed: page.needed
                        currentId: page.moduleId
                        onToggled: id => page.toggled(id)
                        onInspected: id => page.moduleId = id
                    }
                }

                ColumnLayout {
                    width: parent.width
                    visible: page.sections.length === 0
                    spacing: Kirigami.Units.mediumSpacing
                    QQC2.Label {
                        Layout.alignment: Qt.AlignHCenter
                        Layout.topMargin: Kirigami.Units.gridUnit * 2
                        text: page.filter === "selected" && page.query === "" ? "Nothing selected here yet."
                            : "Nothing matches these filters."
                        color: Theme.textMuted
                    }
                    ActionButton {
                        Layout.alignment: Qt.AlignHCenter
                        text: "Show everything"
                        onClicked: {
                            search.text = "";
                            page.hideUnavailable = false;
                            page.safeOnly = false;
                            page.setFilter("all");
                        }
                    }
                }
            }
        }

        // What the last click did; fades after a few seconds.
        Notice {
            id: toastBand
            Layout.fillWidth: true
            visible: opacity > 0
            opacity: toast.running ? 1 : 0
            text: page.feedback.text || ""
            tone: page.feedback.tone || "muted"
            icon: tone === "warning" ? "exchange-positions" : tone === "success" ? "list-add" : "list-remove"
            Behavior on opacity {
                NumberAnimation {
                    duration: 200
                }
            }
            Timer {
                id: toast
                interval: 6000
            }
        }
    }

    ModuleDetails {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 14
        Layout.fillHeight: true
        module: page.current
        selected: page.current !== null && page.selected[page.current.id] === true
        neededBy: page.current !== null ? (page.needed[page.current.id] || "") : ""
    }
}
