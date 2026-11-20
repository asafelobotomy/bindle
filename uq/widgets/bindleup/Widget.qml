// bindleUP's icon in uQ's bar (docs/integrating.md in the uQ repository).
// An arrow up out of a tray, with the number of things ready to update on a badge. With the
// `bar_icon` preference at "updates" (the default) it hides itself while nothing is ready.
// Clicking it lists what is ready, with Update (the update window) and Check now.
// It reads `bindle updates --json` (files only) every minute while its bar shows, and runs a
// full check when the last one is over six hours old, at most every 30 minutes after one fails.
// Every string goes through ctx.UqText as plain text.
import QtQuick

Item {
    id: widget

    property var ctx
    // `bindle updates --json`: {bar_icon, checked, age, stale, count, items: [{title, summary, held}]}.
    property var found: null
    property bool checking: false
    property string problem: ""
    property real lastFailedCheck: 0

    readonly property int count: found ? Number(found.count) || 0 : 0
    readonly property bool always: found !== null && found.bar_icon === "always"
    // Only when updates are ready, unless the preference says always. Until the first answer
    // nothing is known, so it stays out of the way.
    property bool hidden: !(count > 0 || always)
    readonly property bool watching: !ctx || !ctx.view || ctx.view.shown

    function refresh() {
        lastRefresh = Date.now();
        ctx.command.run("state", [], function (r) {
            if (r.error) {
                widget.problem = "Couldn't read what bindleUP found: " + String(r.error);
                return;
            }
            widget.found = r.output;
            if (!widget.checking)
                widget.problem = "";
            if (r.output && r.output.stale === true && !widget.checking
                    && Date.now() - widget.lastFailedCheck > 30 * 60 * 1000)
                widget.check();
        });
    }

    function check() {
        if (checking)
            return;
        checking = true;
        problem = "";
        ctx.command.run("check", [], function (r) {
            widget.checking = false;
            if (r.error) {
                widget.lastFailedCheck = Date.now();
                widget.problem = "The check failed: " + String(r.error);
            }
            widget.refresh();
        });
    }

    function openWindow() {
        ctx.closePopup();
        ctx.command.run("open", [], function (r) {
            widget.problem = r.error ? "Couldn't open bindleUP: " + String(r.error) : "";
        });
    }

    function ago(secs) {
        var s = Number(secs);
        if (!(s >= 0))
            return "";
        if (s < 120)
            return "just now";
        if (s < 7200)
            return Math.floor(s / 60) + " minutes ago";
        if (s < 172800)
            return Math.floor(s / 3600) + " hours ago";
        return Math.floor(s / 86400) + " days ago";
    }

    function headline() {
        if (checking)
            return "Checking for updates…";
        if (!found)
            return "bindleUP";
        if (found.checked === null || found.checked === undefined)
            return "Not checked yet";
        if (count === 0)
            return "Up to date";
        return count === 1 ? "1 update ready" : count + " updates ready";
    }

    property real lastRefresh: 0
    Component.onCompleted: refresh()
    // Back on screen: read again, unless that was just done (showing and hiding the icon itself
    // flips this for a moment).
    onWatchingChanged: if (watching && Date.now() - lastRefresh > 30000) refresh()

    Timer {
        interval: 60000
        running: widget.watching
        repeat: true
        onTriggered: widget.refresh()
    }

    Rectangle {
        anchors.fill: parent
        radius: widget.ctx ? Math.max(0, widget.ctx.Theme.radius - 2) : 4
        color: mouse.containsMouse && widget.ctx ? widget.ctx.Theme.overlay : "transparent"
    }

    // The icon: an arrow up out of a tray.
    Canvas {
        id: icon
        anchors.centerIn: parent
        width: 18
        height: 18
        property color ink: widget.ctx ? (widget.count > 0 ? widget.ctx.Theme.accent : widget.ctx.Theme.text) : "white"
        onInkChanged: requestPaint()
        onPaint: {
            var g = getContext("2d");
            g.reset();
            var c = width / 2;
            g.strokeStyle = String(ink);
            g.lineWidth = 1.6;
            g.lineCap = "round";
            g.lineJoin = "round";
            g.beginPath();
            g.moveTo(c, 12);
            g.lineTo(c, 3);
            g.moveTo(c - 4, 7);
            g.lineTo(c, 3);
            g.lineTo(c + 4, 7);
            g.moveTo(3, 11);
            g.lineTo(3, 15);
            g.lineTo(15, 15);
            g.lineTo(15, 11);
            g.stroke();
        }
    }

    // The badge: how many are ready, or a dot while checking.
    Rectangle {
        visible: widget.count > 0 || widget.checking
        anchors.right: parent.right
        anchors.top: parent.top
        width: widget.checking && widget.count === 0 ? 7 : Math.max(12, badge.implicitWidth + 4)
        height: widget.checking && widget.count === 0 ? 7 : 12
        radius: height / 2
        color: widget.ctx ? (widget.checking ? widget.ctx.Theme.textMuted : widget.ctx.Theme.accent) : "white"
        Loader {
            id: badge
            anchors.centerIn: parent
            active: widget.count > 0
            sourceComponent: widget.ctx ? widget.ctx.UqText : null
            onLoaded: {
                item.baseSize = 8;
                item.baseBold = true;
                item.color = Qt.binding(function () { return widget.ctx.Theme.accentText; });
                item.text = Qt.binding(function () { return widget.count > 9 ? "9+" : String(widget.count); });
            }
        }
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        // Middle click: straight to the update window.
        onClicked: function (m) {
            if (m.button === Qt.MiddleButton) {
                widget.openWindow();
                return;
            }
            widget.refresh();
            widget.ctx.openPopup();
        }
    }

    // What is ready, under the bar icon. It draws with the widget's own ctx (the same one uQ
    // hands the popup), so it has its full size from its first frame.
    property Component popup: Component {
        Item {
            id: pop
            property var ctx: null
            implicitWidth: 320
            implicitHeight: column.implicitHeight + 16

            Column {
                id: column
                x: 8
                y: 8
                width: pop.width - 16
                spacing: 6

                Loader {
                    width: column.width
                    sourceComponent: widget.ctx ? widget.ctx.UqText : null
                    onLoaded: {
                        item.width = Qt.binding(function () { return column.width; });
                        item.baseBold = true;
                        item.text = Qt.binding(function () { return widget.headline(); });
                    }
                }

                Repeater {
                    model: widget.found && Array.isArray(widget.found.items) ? widget.found.items.slice(0, 8) : []
                    delegate: Loader {
                        id: row
                        required property var modelData
                        width: column.width
                        sourceComponent: widget.ctx ? widget.ctx.UqText : null
                        onLoaded: {
                            item.width = Qt.binding(function () { return row.width; });
                            item.wrapMode = Text.Wrap;
                            item.text = String(row.modelData.title) + ": " + String(row.modelData.summary)
                                + (row.modelData.held ? " (held: " + String(row.modelData.held) + ")" : "");
                        }
                    }
                }

                Loader {
                    width: column.width
                    active: widget.problem !== "" || (widget.found !== null && widget.found.age !== null && !widget.checking)
                    visible: active
                    sourceComponent: widget.ctx ? widget.ctx.UqText : null
                    onLoaded: {
                        item.width = Qt.binding(function () { return column.width; });
                        item.wrapMode = Text.Wrap;
                        item.color = Qt.binding(function () {
                            return widget.problem !== "" ? widget.ctx.Theme.error : widget.ctx.Theme.textMuted;
                        });
                        item.text = Qt.binding(function () {
                            return widget.problem !== "" ? widget.problem
                                : "Checked " + widget.ago(widget.found ? widget.found.age : -1);
                        });
                    }
                }

                Row {
                    spacing: 6
                    Repeater {
                        model: [
                            { label: widget.count > 0 ? "Update…" : "Open bindleUP", act: "open" },
                            { label: "Check now", act: "check" }
                        ]
                        delegate: Rectangle {
                            id: button
                            required property var modelData
                            readonly property bool primary: modelData.act === "open" && widget.count > 0
                            readonly property bool usable: !(modelData.act === "check" && widget.checking)
                            width: caption.width + 16
                            height: 24
                            opacity: usable ? 1 : 0.5
                            radius: widget.ctx ? Math.max(0, widget.ctx.Theme.radius - 4) : 4
                            color: !widget.ctx ? "gray"
                                : primary ? widget.ctx.Theme.accent
                                : (press.containsMouse ? widget.ctx.Theme.overlay : widget.ctx.Theme.surfaceAlt)
                            Loader {
                                id: caption
                                anchors.centerIn: parent
                                sourceComponent: widget.ctx ? widget.ctx.UqText : null
                                onLoaded: {
                                    item.text = Qt.binding(function () { return button.modelData.label; });
                                    item.color = Qt.binding(function () {
                                        return button.primary ? widget.ctx.Theme.accentText : widget.ctx.Theme.text;
                                    });
                                }
                            }
                            MouseArea {
                                id: press
                                anchors.fill: parent
                                hoverEnabled: true
                                enabled: button.usable
                                onClicked: {
                                    if (button.modelData.act === "open")
                                        widget.openWindow();
                                    else
                                        widget.check();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
