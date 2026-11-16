// Bindle's page in uQ Settings (docs/settings-pages.md in the uQ repository): the setup Bindle
// last applied, bindleUP (check, update now with progress), what's new, the history of what
// Bindle changed with undo, and bindleUP's preferences. Everything goes through the commands
// declared in page.toml; logic.js reads their answers and words the result. The page never
// forces anything: Bindle's own refusals are shown as they are.
// Laid out like Settings' own pages (uq_api = 3): five tabs of cards, the same names and words
// as the bindle-gui window and the bindleup command.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

SettingsPage {
    id: page

    property var ctx
    property var theme
    /// The [[setting]] row Settings search jumped to; the rows flash, and their tab opens.
    property string highlight
    /// Logic.setupView, Logic.whatsNewView: null until their command answers.
    property var setup: null
    property var whatsNew: null
    /// Logic.historyView, followed (the snapshots folder); null until it answers.
    property var history: null
    /// The key of the row whose write is going, "" when none. One at a time.
    property string workingKey: ""
    /// A check or update that is running or has just finished (Logic.reduce), and whose it is.
    property var progress: null
    property string progressKey: ""
    property bool streaming: false
    /// The last check's plan (Logic.planGroups reads it); null before the first check.
    property var plan: null
    /// How the last update ended (Logic.reduce state, kept when the check after it replaces `progress`).
    property var lastRun: null
    readonly property bool busy: page.workingKey !== "" || page.streaming
    /// What each action said, by row key: {message, failed}.
    property var notes: ({})
    property var openedWith: null

    pageId: "bindle"

    function noteFor(key) {
        return page.notes[key] || null;
    }

    function color(name, fallback) {
        const colors = page.ctx && page.ctx.theme ? page.ctx.theme.colors : null;
        return (colors && colors[name]) || fallback;
    }

    function errorColor() {
        return page.color("error", "red");
    }

    function accentColor() {
        return page.color("accent", "steelblue");
    }

    function textColor() {
        return page.color("text", "black");
    }

    function loadSetup() {
        if (page.ctx)
            page.ctx.command.run("status", [], result => {
                const next = Logic.setupView(result);
                if (next !== null)
                    page.setup = next;
            });
    }

    function loadWhatsNew() {
        if (page.ctx)
            page.ctx.command.run("whats-new", [], result => {
                const next = Logic.whatsNewView(result);
                if (next !== null)
                    page.whatsNew = next;
            });
    }

    function reload() {
        page.loadSetup();
        page.loadWhatsNew();
    }

    /// Runs one action (a command that changes something) for the row `key`, then `done(result)`
    /// unless the person cancelled. `okText` is shown when it worked and said nothing itself.
    function write(key, command, args, okText, done) {
        if (page.busy || !page.ctx)
            return;
        const why = Logic.argsProblem(command, args);
        if (why !== "") {
            page.notes = Logic.withNote(page.notes, key, { message: why, failed: true });
            return;
        }
        page.notes = Logic.withNote(page.notes, key, null);
        page.workingKey = key;
        page.ctx.command.run(command, args, result => {
            page.workingKey = "";
            if (Logic.isCancelled(result))
                return;
            page.notes = Logic.withNote(page.notes, key, Logic.textOutcome(result, okText));
            if (done)
                done(result);
            if (command === "apply-whats-new" || command === "undo")
                page.reload();
        });
    }

    /// Runs a command that streams (check, update): its events drive `progress`, then `done()`.
    function stream(kind, command, args, key, done) {
        if (page.busy || !page.ctx)
            return;
        const why = Logic.argsProblem(command, args);
        if (why !== "") {
            page.notes = Logic.withNote(page.notes, key, { message: why, failed: true });
            return;
        }
        page.notes = Logic.withNote(page.notes, key, null);
        page.progress = Logic.startProgress(kind);
        page.progressKey = key;
        page.streaming = true;
        page.ctx.command.stream(command, args, event => {
            page.progress = Logic.reduce(page.progress, event);
            if (page.progress.plan !== null && kind === "check")
                page.plan = page.progress.plan;
        }, result => {
            page.streaming = false;
            if (Logic.isCancelled(result)) {
                page.progress = null;
                page.progressKey = "";
                return;
            }
            const problem = Logic.streamProblem(page.progress, result);
            if (problem !== "")
                page.notes = Logic.withNote(page.notes, key, { message: problem, failed: true });
            if (done)
                done(result);
        });
    }

    function check() {
        page.stream("check", "check", [], "check", null);
    }

    /// Updates, then looks again: what is left (held, failed) is what the list shows next.
    function update(command, args) {
        page.lastRun = null;
        page.stream("run", command, args, "update", () => {
            page.lastRun = page.progress;
            page.reload();
            page.check();
        });
    }

    onCtxChanged: {
        if (page.ctx && page.ctx !== page.openedWith) {
            page.openedWith = page.ctx;
            page.loadSetup();
            page.loadWhatsNew();
            page.ctx.command.watch("history", result => {
                const next = Logic.historyView(result);
                if (next !== null)
                    page.history = next;
            });
        }
    }

    SettingsTab {
        title: "Setup"
        iconName: "system-software-install"

        SetupTab {
            host: page
            Layout.fillWidth: true
        }
    }

    SettingsTab {
        id: updatesTab
        title: "Updates"
        iconName: "system-software-update"

        UpdatesTab {
            host: page
            Layout.fillWidth: true
        }
    }

    SettingsTab {
        title: "What's new"
        iconName: "view-list-details"

        WhatsNewTab {
            host: page
            Layout.fillWidth: true
        }
    }

    SettingsTab {
        title: "History"
        iconName: "view-history"

        HistoryTab {
            host: page
            Layout.fillWidth: true
        }
    }

    SettingsTab {
        title: "Preferences"
        iconName: "configure"

        PreferencesTab {
            host: page
            Layout.fillWidth: true
        }
    }
}
