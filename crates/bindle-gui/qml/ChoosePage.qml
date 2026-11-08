import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Categories, their modules, and the selected module's details. Typing in
// the search box lists matching modules from every category.
RowLayout {
    id: page

    property var categories: []
    property string notice
    property int categoryIndex: 0
    property string moduleId
    signal toggled(string id)

    readonly property string query: search.text.trim().toLowerCase()
    readonly property var shown: {
        if (query === "")
            return (categories[categoryIndex] || { modules: [] }).modules;
        let out = [];
        for (const c of categories)
            for (const m of c.modules)
                if ((m.name + " " + m.id + " " + m.description).toLowerCase().includes(query))
                    out.push(m);
        return out;
    }
    readonly property var current: {
        for (const c of categories)
            for (const m of c.modules)
                if (m.id === moduleId)
                    return m;
        return shown.length > 0 ? shown[0] : null;
    }

    // For the screenshot run: open a category and (optionally) a module.
    function show(category, module) {
        search.text = "";
        categoryIndex = category;
        const mods = categories[category].modules;
        moduleId = module >= 0 && module < mods.length ? mods[module].id : "";
    }

    spacing: Kirigami.Units.largeSpacing

    ListView {
        id: cats
        Layout.preferredWidth: Kirigami.Units.gridUnit * 10
        Layout.fillHeight: true
        clip: true
        model: page.categories
        spacing: 2
        delegate: QQC2.ItemDelegate {
            id: cat
            required property var modelData
            required property int index
            readonly property bool active: index === page.categoryIndex && page.query === ""
            width: ListView.view.width
            highlighted: active
            onClicked: {
                search.text = "";
                page.categoryIndex = index;
                page.moduleId = "";
            }
            background: Rectangle {
                radius: Theme.radius
                color: cat.active ? Theme.accent : cat.hovered ? Theme.surfaceAlt : "transparent"
            }
            contentItem: RowLayout {
                QQC2.Label {
                    Layout.fillWidth: true
                    text: cat.modelData.name
                    elide: Text.ElideRight
                    color: cat.active ? Theme.accentText : Theme.text
                }
                QQC2.Label {
                    text: cat.modelData.selected + "/" + cat.modelData.total
                    color: cat.active ? Theme.accentText : Theme.textMuted
                    font.pointSize: Theme.smallSize
                }
            }
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: Kirigami.Units.smallSpacing

        QQC2.TextField {
            id: search
            Layout.fillWidth: true
            placeholderText: "Search modules"
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: page.query === "" && page.categories.length > 0
            text: (page.categories[page.categoryIndex] || {}).description || ""
            color: Theme.textMuted
            elide: Text.ElideRight
        }
        ListView {
            id: mods
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: page.shown
            delegate: QQC2.ItemDelegate {
                id: row
                required property var modelData
                readonly property bool provided: modelData.provided !== null && modelData.provided !== undefined
                readonly property bool unavailable: modelData.unavailable !== null && modelData.unavailable !== undefined
                width: ListView.view.width
                onClicked: page.moduleId = modelData.id
                background: Rectangle {
                    radius: Theme.radius
                    color: page.current && page.current.id === row.modelData.id ? Theme.surfaceAlt
                        : row.hovered ? Theme.surface : "transparent"
                }
                contentItem: RowLayout {
                    spacing: Kirigami.Units.mediumSpacing
                    QQC2.CheckBox {
                        checked: row.modelData.selected || row.provided
                        enabled: !row.provided
                        onToggled: {
                            page.moduleId = row.modelData.id;
                            page.toggled(row.modelData.id);
                        }
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: row.modelData.name
                        elide: Text.ElideRight
                        color: row.unavailable ? Theme.textMuted : Theme.text
                    }
                    Pill {
                        visible: row.provided
                        text: "Already set up"
                        tone: "success"
                    }
                    Pill {
                        visible: !row.provided && row.unavailable
                        text: "Not available here"
                        tone: "muted"
                    }
                    Pill {
                        visible: row.modelData.risk !== ""
                        text: row.modelData.risk === "advanced" ? "Advanced" : "Moderate"
                        tone: row.modelData.risk === "advanced" ? "error" : "warning"
                    }
                }
            }
        }
    }

    ModuleDetails {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 15
        Layout.fillHeight: true
        module: page.current
        notice: page.notice
    }
}
