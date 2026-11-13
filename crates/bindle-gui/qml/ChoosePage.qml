import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One category at a time: its groups of modules as cards or rows, with
// search, filters and a view switch in one row above, a card's details
// opening under its group, and a line under the list saying what the last
// click changed.
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
    signal groupSet(var ids, bool on, string group)
    // Categories the user has opened: ticked in the list.
    property var visited: ({})

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
        const v = Object.assign({}, visited);
        v[categoryIndex] = true;
        visited = v;
        moduleId = "";
        freeze();
        toast.stop();
        list.contentItem.contentY = 0;
    }
    // Scroll the list so `item` (opened details) is in view.
    function reveal(item) {
        const flick = list.contentItem;
        const top = item.mapToItem(flick.contentItem, 0, 0).y;
        const bottom = top + item.height + Kirigami.Units.largeSpacing;
        if (bottom > flick.contentY + flick.height)
            flick.contentY = Math.min(bottom - flick.height, Math.max(0, flick.contentHeight - flick.height));
        else if (top < flick.contentY)
            flick.contentY = top;
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
                QQC2.Label {
                    visible: !cat.active && page.visited[cat.index] === true
                    text: "✓"
                    color: Theme.accent
                    font.pointSize: Theme.smallSize
                    Accessible.name: "visited"
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

        }

        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            QQC2.TextField {
                id: search
                width: Math.max(Kirigami.Units.gridUnit * 9, parent.width - filters.width - more.width - views.width - 3 * parent.spacing - 1)
                placeholderText: "Search every category"
                onTextChanged: list.contentItem.contentY = 0
            }
            Row {
                id: filters
                spacing: 2
                Repeater {
                    model: [["all", "All"], ["selected", "Selected"], ["suggested", "Recommended"]]
                    Chip {
                        required property var modelData
                        text: modelData[1]
                        count: page.size(page.sectionsFor(modelData[0]))
                        active: page.filter === modelData[0]
                        onClicked: page.setFilter(modelData[0])
                    }
                }
            }
            Chip {
                id: more
                text: "More filters" + (page.hideUnavailable || page.safeOnly ? " ●" : "") + " ▾"
                active: menu.visible
                onClicked: menu.open()
                QQC2.Menu {
                    id: menu
                    y: more.height
                    QQC2.MenuItem {
                        text: "Hide what isn't available here"
                        checkable: true
                        checked: page.hideUnavailable
                        onTriggered: page.hideUnavailable = checked
                    }
                    QQC2.MenuItem {
                        text: "Only safe changes (no Moderate or Advanced)"
                        checkable: true
                        checked: page.safeOnly
                        onTriggered: page.safeOnly = checked
                    }
                }
            }
            Row {
                id: views
                spacing: 2
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
                        onInspected: id => page.moduleId = page.moduleId === id ? "" : id
                        onSetAll: (ids, on) => page.groupSet(ids, on, modelData.title)
                        // After layout, so the details have their height.
                        onDetailsShown: item => revealTimer.reveal(item)
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

        Timer {
            id: revealTimer
            property var item: null
            function reveal(it) {
                item = it;
                restart();
            }
            interval: 50
            onTriggered: if (item) page.reveal(item)
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
}
