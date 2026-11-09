import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One catalog category of the plan: its icon, name and modules. On Review
// the modules show as chips (the first few, then "+N"); once applied
// (`done`) a tick and the count.
Panel {
    id: card

    property string categoryId
    property string name
    property var modules: []
    property bool done: false
    readonly property int shown: 3

    // Breeze icon names, outline style (Glyph draws them in one colour; a
    // full-colour icon would become a solid square). A missing one draws nothing.
    readonly property var icons: ({
        base: "configure", hardware: "computer", performance: "speedometer",
        desktop: "view-list-icons", internet: "applications-internet",
        creative: "draw-brush", development: "format-text-code",
        ai: "tools-wizard", gaming: "input-gamepad", security: "security-high"
    })

    padding: Kirigami.Units.mediumSpacing
    implicitHeight: body.implicitHeight + 2 * padding

    ColumnLayout {
        id: body
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Glyph {
                name: card.done ? "dialog-ok-apply" : (card.icons[card.categoryId] || "")
                tint: card.done ? Theme.success : Theme.accent
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: card.name
                color: Theme.text
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
            QQC2.Label {
                visible: !card.done
                text: card.modules.length
                color: Theme.textMuted
                font.pointSize: Theme.smallSize
            }
        }

        QQC2.Label {
            visible: card.done
            text: card.modules.length === 1 ? "1 feature" : card.modules.length + " features"
            color: Theme.textMuted
            font.pointSize: Theme.smallSize
        }

        Flow {
            Layout.fillWidth: true
            visible: !card.done
            spacing: Kirigami.Units.smallSpacing
            Repeater {
                model: card.modules.slice(0, card.shown)
                Pill {
                    required property string modelData
                    text: modelData
                    tone: "accent"
                }
            }
            Pill {
                visible: card.modules.length > card.shown
                text: "+" + (card.modules.length - card.shown)
            }
        }
    }

    QQC2.ToolTip.visible: hover.hovered && card.modules.length > (card.done ? 0 : card.shown)
    QQC2.ToolTip.text: card.modules.join("\n")
    HoverHandler {
        id: hover
    }
}
