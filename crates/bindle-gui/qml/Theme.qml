pragma Singleton
import QtQuick

// Every colour, radius and font the views use. On uQ these come from uQ's
// theme tokens (set by Main from backend.themeJson); elsewhere from the
// desktop's palette, so the window looks native there.
QtObject {
    id: theme

    property var tokens: ({ present: false })
    readonly property bool uq: tokens.present === true
    readonly property SystemPalette sys: SystemPalette { colorGroup: SystemPalette.Active }
    readonly property bool dark: (sys.window.r + sys.window.g + sys.window.b) / 3 < 0.5

    function pick(key, fallback) {
        return uq ? tokens.colors[key] : fallback;
    }

    readonly property color bg: pick("bg", sys.window)
    readonly property color surface: pick("surface", sys.base)
    readonly property color surfaceAlt: pick("surface_alt", sys.alternateBase)
    readonly property color overlay: pick("overlay", sys.midlight)
    readonly property color text: pick("text", sys.windowText)
    readonly property color textMuted: pick("text_muted", sys.placeholderText)
    readonly property color accent: pick("accent", sys.highlight)
    readonly property color accentText: pick("accent_text", sys.highlightedText)
    readonly property color border: pick("border", sys.mid)
    readonly property color success: pick("success", dark ? "#81c995" : "#1e7a3a")
    readonly property color warning: pick("warning", dark ? "#fdd663" : "#8a6100")
    readonly property color error: pick("error", dark ? "#f28b82" : "#b3261e")

    readonly property int radius: uq ? tokens.radius : 6
    readonly property string fontUi: uq ? tokens.fontUi : ""
    readonly property string fontMono: uq && tokens.fontMono ? tokens.fontMono : "monospace"
    readonly property int fontSize: uq ? tokens.fontSize : 0
    // One scale for every screen: uQ's size, else the desktop's.
    readonly property real bodySize: fontSize > 0 ? fontSize : Qt.application.font.pointSize
    readonly property real smallSize: Math.max(7, bodySize - 1)
    readonly property real headingSize: Math.round(bodySize * 1.5)
}
