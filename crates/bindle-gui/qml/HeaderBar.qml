import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

// The header: the Bindle name, then the window's own items.
WindowBar {
    spacing: Kirigami.Units.mediumSpacing

    QQC2.Label {
        text: "Bindle"
        font.weight: Font.DemiBold
        color: Theme.accent
    }
}
