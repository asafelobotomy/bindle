pragma Singleton
import QtQuick

// Every colour, radius and font the views use. On uQ these come from uQ's
// theme tokens (set by Main from backend.themeJson); elsewhere from the
// window's Kirigami.Theme (Main sets `win` and `view`), the colours the
// desktop style draws controls with, so fields and check boxes match the page.
QtObject {
    id: theme

    property var tokens: ({ present: false })
    readonly property bool uq: tokens.present === true
    // Kirigami.Theme of the window (Window colour set) and of a View item.
    property QtObject win: null
    property QtObject view: null
    // The Backend (set by Main), for glyph(): theme icons in one colour.
    property QtObject backend: null

    function pick(key, fallback) {
        return uq ? tokens.colors[key] : fallback;
    }
    function sys(set, key, fallback) {
        return set ? set[key] : fallback;
    }

    readonly property color bg: pick("bg", sys(win, "backgroundColor", "#eff0f1"))
    readonly property color surface: pick("surface", sys(view, "backgroundColor", "#ffffff"))
    readonly property color surfaceAlt: pick("surface_alt", sys(view, "alternateBackgroundColor", "#f7f7f7"))
    readonly property color overlay: pick("overlay", sys(win, "hoverColor", "#93cee9"))
    readonly property color text: pick("text", sys(win, "textColor", "#232627"))
    readonly property color textMuted: pick("text_muted", sys(win, "disabledTextColor", "#707d8a"))
    readonly property color accent: pick("accent", sys(win, "highlightColor", "#3daee9"))
    readonly property color accentText: pick("accent_text", sys(win, "highlightedTextColor", "#ffffff"))
    readonly property color border: pick("border", Qt.tint(bg, Qt.alpha(text, 0.2)))
    readonly property color success: pick("success", sys(win, "positiveTextColor", "#27ae60"))
    readonly property color warning: pick("warning", sys(win, "neutralTextColor", "#f67400"))
    readonly property color error: pick("error", sys(win, "negativeTextColor", "#da4453"))

    readonly property int radius: uq ? tokens.radius : 6
    readonly property string fontUi: uq ? tokens.fontUi : ""
    readonly property string fontMono: uq && tokens.fontMono ? tokens.fontMono : "monospace"
    readonly property int fontSize: uq ? tokens.fontSize : 0
    // One scale for every screen: uQ's size, else the desktop's.
    readonly property real bodySize: fontSize > 0 ? fontSize : Qt.application.font.pointSize
    readonly property real smallSize: Math.max(7, bodySize - 1)
    readonly property real headingSize: Math.round(bodySize * 1.5)
}
