// A check or an update that is running, or has just finished: a bar that follows the events
// (Logic.reduce), the line saying what is happening, and what each stage answered.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: view

    property var host
    /// Which run this view shows: the page's progressKey ("check" or "update").
    property string progressKey
    /// Show each stage's result (an update), not only the line.
    property bool showResults: true
    readonly property var state: view.host && view.host.progressKey === view.progressKey ? view.host.progress : null

    visible: view.state !== null
    spacing: 4

    QQC2.ProgressBar {
        visible: view.state !== null && !view.state.done && !(view.state.kind === "check" && view.state.plan !== null)
        indeterminate: view.state !== null && Logic.fraction(view.state) < 0
        value: view.state !== null ? Math.max(0, Logic.fraction(view.state)) : 0
        Layout.fillWidth: true
    }
    UqLabel {
        text: view.state !== null ? Logic.progressLine(view.state) : ""
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
    Repeater {
        model: view.showResults && view.state !== null ? view.state.results : []

        UqLabel {
            id: line

            required property var modelData

            text: line.modelData.title + ": " + Logic.outcomeWord(line.modelData.outcome) + (line.modelData.why !== "" ? " (" + line.modelData.why + ")" : "")
            wrapMode: Text.Wrap
            color: line.modelData.outcome === "failed" && view.host ? view.host.errorColor() : (view.host ? view.host.textColor() : "black")
            opacity: line.modelData.outcome === "failed" ? 1 : 0.8
            Layout.fillWidth: true
        }
    }
}
