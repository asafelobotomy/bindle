import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.bindle.wizard

ThemedWindow {
    id: root

    // 0 Welcome, 1 Profile, 2 Choose (one category at a time), 3 Summary,
    // 4 Review, 5 Apply, 6 What's new (after a Bindle upgrade: then the steps
    // are What's new → Review → Apply, over its picks).
    property int page: 0
    readonly property var whatsNew: JSON.parse(backend.whatsNewJson || '{"state": "none"}')
    readonly property bool whatsNewMode: whatsNew.state === "ready" && whatsNew.active === true
    readonly property var modePages: [6, 4, 5]
    // The furthest screen visited: the step bar goes back to any of them.
    property int reached: 0
    readonly property var apply: JSON.parse(backend.applyJson || '{"state": "idle"}')
    readonly property bool running: apply.state === "running"
    property string footerNote: ""
    readonly property var steps: whatsNewMode ? ["What's new", "Review", "Apply"]
        : ["Welcome", "Profile", "Choose", "Summary", "Review", "Apply"]
    readonly property var catalog: JSON.parse(backend.catalogJson || "[]")
    readonly property var selection: JSON.parse(backend.stateJson || "{}")
    readonly property int picked: selection.total || 0
    readonly property var welcome: JSON.parse(backend.welcomeJson || "{}")

    title: "Bindle"
    width: Kirigami.Units.gridUnit * 60
    height: Kirigami.Units.gridUnit * 36
    minimumWidth: Kirigami.Units.gridUnit * 38
    minimumHeight: Kirigami.Units.gridUnit * 26
    themeBackend: backend

    Backend {
        id: backend
    }

    Component.onCompleted: {
        if (root.whatsNewMode)
            root.page = 6;
    }

    // Leave What's new for the wizard: "Not now" (it asks again next time),
    // or `skip` this version (it opens by itself again for a newer Bindle).
    function leaveWhatsNew(skip) {
        if (skip)
            backend.skipWhatsNew();
        else
            backend.setWhatsNew(false);
        root.reached = 0;
        root.go(0);
    }

    function go(to) {
        root.footerNote = "";
        if (to === 4)
            backend.refreshReview();
        root.page = Math.max(0, Math.min(6, to));
        root.reached = Math.max(root.reached, root.page);
    }
    // The step bar: going back to Choose resumes where the user was.
    function jump(to) {
        if (root.whatsNewMode)
            return root.go(root.modePages[to]);
        if (to === 2)
            choose.open(choose.categoryIndex);
        root.go(to);
    }
    // Where the primary button leads, as its label.
    function nextLabel() {
        if (root.page === 6)
            return root.whatsNew.picked > 0 ? "Review " + root.whatsNew.picked + (root.whatsNew.picked === 1 ? " pick" : " picks")
                : "Continue to setup";
        switch (root.page) {
        case 0:
            return "Get started";
        case 1:
            return "Continue";
        case 2:
            return choose.last ? "See your selections" : "Next: " + choose.nextName;
        case 3:
            return "Go to final review";
        case 4:
            return "Apply changes";
        }
        if (root.apply.state === "done")
            return "Close";
        return root.apply.noAgent ? "Continue in a terminal" : "Try again";
    }
    function next() {
        if (root.page === 6) {
            if (root.whatsNew.picked > 0)
                root.go(4);
            else
                root.leaveWhatsNew();
            return;
        }
        if (root.page === 1) {
            choose.open(0);
            root.go(2);
        } else if (root.page === 2 && !choose.last) {
            choose.open(choose.categoryIndex + 1);
        } else if (root.page < 4) {
            root.go(root.page + 1);
        } else if (root.page === 4 || (root.apply.state !== "done" && !root.apply.noAgent)) {
            root.go(5);
            backend.startApply();
        } else if (root.apply.noAgent) {
            backend.continueInTerminal();
        } else {
            Qt.quit();
        }
    }
    function back() {
        if (root.whatsNewMode && root.page === 4) {
            root.go(6);
            return;
        }
        if (root.page === 2 && choose.categoryIndex > 0 && choose.query === "")
            choose.open(choose.categoryIndex - 1);
        else
            root.go(root.page - 1);
    }

    header: HeaderBar {
        Item {
            Layout.fillWidth: true
        }
        StepBar {
            steps: root.steps
            current: root.whatsNewMode ? root.modePages.indexOf(root.page) : root.page
            // Apply is reached only through Review's button.
            reached: root.whatsNewMode ? (root.apply.state === "idle" ? Math.min(1, root.reached >= 4 ? 1 : 0) : 2)
                : Math.min(root.reached, root.apply.state === "idle" ? 4 : 5)
            detail: root.page === 2 && choose.query === "" ? (choose.categoryIndex + 1) + "/" + root.catalog.length : ""
            locked: root.running || root.apply.state === "done"
            onPicked: index => root.jump(index)
        }
    }

    StackLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        currentIndex: root.page

        WelcomePage {
            summary: root.welcome
            machine: backend.header
            categories: root.catalog
            selection: root.selection
            onOpen: index => {
                choose.open(index);
                root.go(2);
            }
        }
        ProfilePage {
            profiles: JSON.parse(backend.profilesJson || "{}")
            onChosen: id => backend.chooseProfile(id)
            onAddon: id => backend.toggleAddon(id)
        }
        ChoosePage {
            id: choose
            categories: root.catalog
            selection: root.selection
            feedback: JSON.parse(backend.feedbackJson || "{}")
            onToggled: id => backend.toggle(id)
            onGroupSet: (ids, on, group) => backend.setGroup(JSON.stringify(ids), on, group)
        }
        SummaryPage {
            categories: root.catalog
            selection: root.selection
            alreadySetUp: (root.welcome.working || []).length
            onEdit: index => {
                choose.open(index);
                root.go(2);
            }
            onRemove: id => backend.toggle(id)
        }
        ReviewPage {
            id: reviewPage
            review: JSON.parse(backend.reviewJson || "{}")
        }
        ApplyPage {
            id: applyPage
            apply: root.apply
            review: reviewPage.review
            onRestart: backend.reboot()
            onUndo: backend.undoInTerminal()
            onOpenLog: path => Qt.openUrlExternally("file://" + path)
        }
        WhatsNewPage {
            id: whatsNewPage
            whatsNew: root.whatsNew
            onToggled: id => backend.toggleWhatsNew(id)
            onChangeToggled: (id, key) => backend.toggleWhatsNewChange(id, key)
            onHidden: id => backend.hideWhatsNew(id)
        }
    }

    footer: WindowBar {
        footer: true

        ActionButton {
            text: root.page === 5 ? "Back to review" : "Back"
            visible: root.page > 0 && root.page !== 6 && !root.running && root.apply.state !== "done"
            onClicked: root.back()
        }
        ActionButton {
            visible: root.page === 6
            text: "Not now"
            onClicked: root.leaveWhatsNew()
        }
        ActionButton {
            visible: root.page === 6
            text: "Skip this version"
            onClicked: root.leaveWhatsNew(true)
        }
        QQC2.Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignRight
            text: root.running ? "Keep the computer on until this finishes." : (backend.notice !== "" && root.page === 5 ? backend.notice
                : root.page === 2 || root.page === 3 ? root.picked + (root.picked === 1 ? " feature selected" : " features selected")
                    + (Object.keys(root.selection.needed || {}).length > 0 ? " · " + Object.keys(root.selection.needed).length + " added for them" : "")
                : root.footerNote)
            color: Theme.textMuted
            elide: Text.ElideLeft
        }
        ActionButton {
            visible: root.page === 2 && !choose.last
            text: "Skip to summary"
            onClicked: root.go(3)
        }
        ActionButton {
            visible: !root.running
            primary: true
            text: root.nextLabel()
            onClicked: root.next()
        }
    }

    // `--screenshots <dir>`: show each screen, save it, then quit.
    ShotDriver {
        id: shots

        appBackend: backend
        busy: root.running
        plan: [
            { page: 0, name: "1-welcome" },
            { page: 1, name: "2-profile" },
            { page: 2, name: "3-choose-grid", category: 0, view: "grid" },
            // A category with "choose one" sets, in each view.
            { page: 2, name: "3b-choose-one", category: 2, view: "grid", toggle: "earlyoom" },
            { page: 2, name: "3c-choose-list", category: 4, view: "list", toggle: "brave" },
            { page: 2, name: "3d-choose-compact", category: 6, view: "compact" },
            { page: 2, name: "3e-choose-selected", category: 1, view: "grid", filter: "selected" },
            { page: 3, name: "4-summary" },
            { page: 4, name: "5-review" },
            { page: 4, name: "5b-review-details", details: true },
            // What's new after a Bindle upgrade, on a made-up comparison.
            { page: 6, name: "7-whats-new", whatsNew: true },
            { page: 6, name: "7b-whats-new-changes", whatsNewOpen: true },
            // Only with --sandbox: apply for real (recording commands) and wait.
            { page: 5, name: "6-applied", apply: true },
            { page: 5, name: "6b-applied-details", details: true }
        ]
        onShoot: s => {
            if (s.apply && backend.sandbox === "") {
                shots.stop();
                Qt.quit();
                return;
            }
            if (s.apply)
                backend.setWhatsNew(false);
            if (s.whatsNew)
                backend.sampleWhatsNew();
            if (s.whatsNewOpen) {
                whatsNewPage.openAll = true;
                Qt.callLater(() => whatsNewPage.contentItem.contentY = Kirigami.Units.gridUnit * 13);
            }
            if (s.details) {
                // Open the current page's details, scrolled to show them.
                if (s.page === 4) {
                    reviewPage.detailsOpen = true;
                    reviewPage.showDetails();
                } else {
                    applyPage.detailsOpen = true;
                }
            } else {
                root.go(s.page);
            }
            if (s.apply) {
                backend.startApply();
                shots.waiting = true;
            }
            if (s.category !== undefined) {
                choose.open(s.category);
                choose.view = s.view;
                if (s.toggle)
                    backend.toggle(s.toggle);
                choose.setFilter(s.filter || "all");
                choose.moduleId = s.toggle || "";
            }
        }
    }
}
