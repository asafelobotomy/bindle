// uq-consts 1.0.0
// The base of a uQ native app's QML Theme singleton. Generated from uQ's
// native/uq-consts/consts.toml by gen.py: vendor it unchanged next to your Theme.qml and
// register it in the same QML module. Then Theme.qml is:
//
//     pragma Singleton
//     import QtQuick
//     UqThemeBase { tokens: App.theme.tokens }   // uq-theme's tokens, plus your own extras
//
// On uQ every value follows uQ's theme (and Reduce motion) live; elsewhere it is uQ's
// Graphite palette with the app font, so the app looks the same as uQ's own apps.
import QtQuick

QtObject {
    // uq-theme's tokens (load() / Watcher.tokens); {present: false} off uQ.
    property var tokens: ({ present: false })
    // Off uQ: colours to use instead of Graphite's, for an app that follows the desktop's own
    // palette there (for example Kirigami.Theme), as {bg: ..., accent: ...}. Missing keys
    // stay Graphite. Don't derive them from this object's own colours (a binding loop).
    property var fallbackColors: ({})
    property int fallbackRadius: 10
    readonly property bool uq: !!tokens && tokens.present === true
    // On uQ the theme's variant; elsewhere whether the background is dark.
    readonly property bool dark: uq ? tokens.variant !== "light" : bg.hslLightness < 0.5

    function pick(key, fallback) {
        if (uq && tokens.colors && tokens.colors[key])
            return tokens.colors[key];
        return fallbackColors && fallbackColors[key] ? fallbackColors[key] : fallback;
    }
    function num(key, fallback) {
        return uq && typeof tokens[key] === "number" ? tokens[key] : fallback;
    }

    readonly property color bg: pick("bg", "#1f1f22")
    readonly property color surface: pick("surface", "#2b2b2e")
    readonly property color surfaceAlt: pick("surface_alt", "#353538")
    readonly property color overlay: pick("overlay", "#464649")
    readonly property color text: pick("text", "#e4e4e7")
    readonly property color textMuted: pick("text_muted", "#a9a9ac")
    readonly property color accent: pick("accent", "#8ab4f8")
    readonly property color accentText: pick("accent_text", "#1f1f22")
    readonly property color border: pick("border", "#5a5a5d")
    readonly property color success: pick("success", "#81c995")
    readonly property color warning: pick("warning", "#fdd663")
    readonly property color error: pick("error", "#f28b82")

    // Corners: containers use radius, a row, button or field inside one radiusControl, a chip
    // or thumbnail inside a row radiusInner, so nested corners stay concentric.
    readonly property int radius: num("radius", fallbackRadius)
    readonly property int radiusControl: Math.max(0, radius - 2)
    readonly property int radiusInner: Math.max(0, radius - 4)
    readonly property int gapIn: num("gapIn", 5)
    readonly property int gapOut: num("gapOut", 12)
    readonly property int borderWidth: num("borderWidth", 1)
    readonly property int gap: 8

    readonly property string fontUi: uq && tokens.fontUi ? tokens.fontUi : Qt.application.font.family
    readonly property string fontMono: uq && tokens.fontMono ? tokens.fontMono : "monospace"
    readonly property real bodySize: uq && tokens.fontSize > 0 ? tokens.fontSize
        : (Qt.application.font.pointSize > 0 ? Qt.application.font.pointSize : 11)
    readonly property real smallSize: Math.max(8, bodySize - 1)
    readonly property real headingSize: Math.round(bodySize * 1.5)

    // Motion: the same speeds and curves as uQ's shell. Use these, not literal ms. All 0 when
    // the user turned on Reduce motion in uQ.
    readonly property var motion: uq && tokens.motion ? tokens.motion : ({})
    readonly property bool reduceMotion: motion.reduce === true
    readonly property int motionFast: reduceMotion ? 0 : (motion.fast >= 0 ? motion.fast : 100)
    readonly property int motionBase: reduceMotion ? 0 : (motion.base >= 0 ? motion.base : 140)
    readonly property int motionSlow: reduceMotion ? 0 : (motion.slow >= 0 ? motion.slow : 220)
    readonly property int easing: Easing.OutCubic
    readonly property int easingExit: Easing.InCubic
    readonly property real popScale: reduceMotion ? 1 : 0.96
}
