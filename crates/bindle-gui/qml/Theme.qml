pragma Singleton
import QtQuick

// Every colour, radius, font size and speed the views use: uQ's shared theme base
// (UqThemeBase.qml, vendored unchanged from uQ's native/uq-consts). On uQ it follows uQ's theme
// tokens (set by ThemedWindow from backend.themeJson) and Reduce motion; elsewhere the colours
// are the window's Kirigami.Theme (ThemedWindow sets `win` and `view`), the colours the desktop
// style draws controls with, so fields and check boxes match the page.
UqThemeBase {
    // Kirigami.Theme of the window (Window colour set) and of a View item.
    property QtObject win: null
    property QtObject view: null
    // The Backend (set by ThemedWindow), for Glyph: theme icons in one colour.
    property QtObject backend: null

    function sys(set, key, fallback) {
        return set ? set[key] : fallback;
    }

    fallbackRadius: 6
    // Off uQ: the desktop's colours. Built from Kirigami only (not this object's own colours).
    fallbackColors: {
        const winBg = sys(win, "backgroundColor", "#eff0f1");
        const winText = sys(win, "textColor", "#232627");
        return {
            bg: winBg,
            surface: sys(view, "backgroundColor", "#ffffff"),
            surface_alt: sys(view, "alternateBackgroundColor", "#f7f7f7"),
            overlay: sys(win, "hoverColor", "#93cee9"),
            text: winText,
            text_muted: sys(win, "disabledTextColor", "#707d8a"),
            accent: sys(win, "highlightColor", "#3daee9"),
            accent_text: sys(win, "highlightedTextColor", "#ffffff"),
            border: Qt.tint(winBg, Qt.alpha(winText, 0.2)),
            success: sys(win, "positiveTextColor", "#27ae60"),
            warning: sys(win, "neutralTextColor", "#f67400"),
            error: sys(win, "negativeTextColor", "#da4453"),
        };
    }

    // Breeze icon names per catalog category, outline style (Glyph draws them
    // in one colour; a full-colour icon would become a solid square).
    readonly property var categoryIcons: ({
        base: "configure", hardware: "computer", performance: "speedometer",
        desktop: "view-list-icons", internet: "applications-internet",
        creative: "draw-brush", development: "format-text-code",
        ai: "tools-wizard", gaming: "input-gamepad", security: "security-high"
    })
}
