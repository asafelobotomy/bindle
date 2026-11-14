import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.bindle.wizard

// A Bindle window: uQ's theme (or the desktop's) on the window, its font
// and the palette the Qt Quick controls draw from. `themeBackend` is the
// window's backend: its `themeJson` feeds Theme.
QQC2.ApplicationWindow {
    id: win

    required property QtObject themeBackend

    visible: true
    color: Theme.bg
    font.family: Theme.fontUi !== "" ? Theme.fontUi : Qt.application.font.family
    font.pointSize: Theme.bodySize

    // Fusion and the Qt Quick controls draw from the palette: feed it the theme.
    palette {
        window: Theme.bg
        windowText: Theme.text
        base: Theme.surface
        alternateBase: Theme.surfaceAlt
        text: Theme.text
        button: Theme.surfaceAlt
        buttonText: Theme.text
        highlight: Theme.accent
        highlightedText: Theme.accentText
        placeholderText: Theme.textMuted
        mid: Theme.border
        dark: Theme.border
        light: Theme.overlay
        midlight: Theme.overlay
    }

    Binding {
        target: Theme
        property: "tokens"
        value: JSON.parse(win.themeBackend.themeJson || '{"present": false}')
    }

    // Off uQ, Theme takes the desktop's colours from Kirigami, which the
    // desktop style also draws controls with.
    Item {
        id: winColors
        Kirigami.Theme.colorSet: Kirigami.Theme.Window
        Kirigami.Theme.inherit: false
        Item {
            id: viewColors
            Kirigami.Theme.colorSet: Kirigami.Theme.View
            Kirigami.Theme.inherit: false
        }
    }
    Component.onCompleted: {
        Theme.win = winColors.Kirigami.Theme;
        Theme.view = viewColors.Kirigami.Theme;
        Theme.backend = win.themeBackend;
    }
}
