// uQ Settings' "Re-run setup" page: the setup Bindle last applied, what of it is
// missing now, and Bindle's app picker. All text through UqLabel (docs/settings-pages.md).
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings

Item {
    id: page

    property var ctx
    property string highlight
    property var status: null
    property string problem
    property bool checking: false
    property bool opened: false

    function color(name, fallback) {
        const c = page.ctx && page.ctx.theme && page.ctx.theme.colors;
        return (c && c[name]) || fallback;
    }

    function when(secs) {
        if (!secs)
            return "";
        return new Date(secs * 1000).toLocaleString(Qt.locale(), Locale.LongFormat);
    }

    function check() {
        if (!page.ctx || page.checking)
            return;
        page.checking = true;
        page.ctx.command.run("status", [], result => {
            page.checking = false;
            if (result.error) {
                page.problem = "Bindle couldn't check this system: " + result.error;
                return;
            }
            page.problem = "";
            page.status = result.output;
        });
    }

    function pickApps() {
        page.ctx.command.run("open", [], result => {
            if (result.error) {
                page.problem = "Bindle couldn't open: " + result.error;
                return;
            }
            page.problem = "";
            page.opened = true;
        });
    }

    onCtxChanged: check()

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        UqLabel {
            text: "Bindle set up this system's apps and settings. Pick apps again to add or remove some, or check whether anything it installed has gone missing."
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }

        UqLabel {
            visible: page.status !== null && !page.status.selection
            text: "Bindle hasn't set up anything for you yet."
            Layout.fillWidth: true
        }
        UqLabel {
            visible: page.status !== null && !!page.status.selection
            text: "Profile: " + ((page.status && page.status.profile) || "none")
            Layout.fillWidth: true
        }
        UqLabel {
            visible: page.status !== null && !!page.status.last_applied
            text: "Last applied: " + page.when(page.status ? page.status.last_applied : 0)
            Layout.fillWidth: true
        }

        RowLayout {
            spacing: 8
            QQC2.Button {
                id: pick
                text: "Pick apps again"
                highlighted: page.highlight === "pick-apps"
                onClicked: page.pickApps()
            }
            QQC2.Button {
                text: page.checking ? "Checking…" : "Check for missing pieces"
                enabled: !page.checking
                highlighted: page.highlight === "missing"
                onClicked: page.check()
            }
        }
        UqLabel {
            visible: page.opened
            text: "Bindle is opening in its own window."
            color: page.color("text_muted", "gray")
            Layout.fillWidth: true
        }

        UqLabel {
            visible: page.status !== null && !!page.status.selection && !page.status.error && page.status.missing.length === 0
            text: "Everything Bindle set up is still in place."
            Layout.fillWidth: true
        }
        UqLabel {
            visible: page.status !== null && !!page.status.selection && !page.status.error && page.status.missing.length > 0
            text: "Not in place now. Applying your setup again (Pick apps again) would:"
            Layout.fillWidth: true
        }
        Repeater {
            model: (page.status && !page.status.error && page.status.missing) || []
            delegate: UqLabel {
                required property string modelData
                text: "• " + modelData
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
        }

        UqLabel {
            visible: page.problem !== "" || !!(page.status && page.status.error)
            text: page.problem !== "" ? page.problem : ("The saved setup can't be checked: " + ((page.status && page.status.error) || ""))
            color: page.color("error", "red")
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }

        Item {
            Layout.fillHeight: true
        }
    }
}
