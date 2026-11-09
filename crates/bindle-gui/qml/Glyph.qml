import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A theme icon drawn as one flat colour (`tint`), so it reads on any
// background: Backend.glyph recolours the icon's SVG (src/glyph.rs).
// Hidden when no installed icon theme has the name.
Image {
    id: glyph

    property string name
    property color tint: Theme.text
    property int size: Kirigami.Units.iconSizes.small

    // "#rrggbb", whatever the tint's alpha.
    readonly property string hex: "#" + ((1 << 24) + (Math.round(tint.r * 255) << 16)
        + (Math.round(tint.g * 255) << 8) + Math.round(tint.b * 255)).toString(16).slice(1)

    source: Theme.backend && glyph.name !== "" ? Theme.backend.glyph(glyph.name, glyph.hex) : ""
    visible: status === Image.Ready
    sourceSize: Qt.size(size * Screen.devicePixelRatio, size * Screen.devicePixelRatio)
    width: size
    height: size
    Layout.preferredWidth: size
    Layout.preferredHeight: size
    fillMode: Image.PreserveAspectFit
    smooth: true
}
