// Loads the uQ Settings page (uq/pages/bindle) with a fake ctx, as uQ Settings would, and checks
// what it shows and the commands it runs. Run with:
//   QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -import tests/uq/stub -input tests/uq/tst_page.qml
import QtQuick
import QtQuick.Controls as QQC2
import QtTest

Item {
    id: root
    width: 900
    height: 700

    readonly property var plan: ({
            event: "plan",
            work: 2,
            stages: [
                { id: "bindle", title: "Bindle", state: "current", summary: "0.1.11 is the latest", why: "", details: [], needs_root: false },
                { id: "news", title: "Arch news", state: "note", summary: "1 unread post", why: "", details: ["2026-10-21 Update php"], needs_root: false },
                { id: "pacman", title: "Packages", state: "run", summary: "2 packages to upgrade", why: "", details: ["linux 6.1 → 6.2", "mesa 25.0 → 25.1"], needs_root: true },
                { id: "flatpak", title: "Flatpak", state: "run", summary: "1 update", why: "", details: ["org.gimp.GIMP"], needs_root: false },
                { id: "aur", title: "AUR", state: "held", summary: "held", why: "the system upgrade has to run first", details: [], needs_root: false }
            ]
        })

    readonly property var base: ({
            status: { code: 0, output: { selection: "/home/u/.config/bindle/selection.toml", profile: "desktop", with: ["fstrim"], without: ["cups"], last_applied: 1760000000, missing: ["install package zram-generator", "sh -ec for f in /etc/pam.d/greetd; do\n  grep -q x $f\ndone"], groups: [{ kind: "packages", label: "Install 1 package", items: ["zram-generator"] }, { kind: "commands", label: "Run 1 setup command", items: ["Unlock KDE Wallet at login"] }], error: null } },
            "whats-new": { code: 0, output: { version: "0.1.12", since: "0.1.11", recorded: true, items: [
                        { id: "zram", name: "zram swap", kind: "updated", why: "changed since it was set up", ticked: true, changes: [{ text: "~ changes /etc/systemd/zram-generator.conf" }] },
                        { id: "kyko", name: "kyko", kind: "optional", why: "new", ticked: false, changes: [] }
                    ] } },
            history: { code: 0, output: { snapshots: [
                        { id: "20260101-1", created: "2026-01-01", label: "zram, fstrim", version: "0.1.11", files: 3, units: 1, packages: 2, undone: false, complete: true },
                        { id: "20251201-1", created: "2025-12-01", label: "base", version: "0.1.10", files: 1, units: 0, packages: 0, undone: true, complete: false }
                    ] } },
            config: { code: 0, output: { path: "/home/u/.config/bindle/upgrade.toml", update_notify: false, apps: [{ app: "kyko", value: "bindle", source: "user" }], keys: [
                        { key: "news_days", group: "Checks", label: "Arch news window", help: "h", kind: "int", value: 30, default: 30, min: 0, max: 3650, unit: "days", source: "default" },
                        { key: "space.boot_mb", group: "Free space", label: "Needed on /boot", help: "h", kind: "int", value: 500, default: 200, min: 0, max: 1000000, unit: "MB", source: "user" },
                        { key: "releases.follow", group: "Apps", label: "Where pinned apps update from", help: "h", kind: "choice", value: "source", default: "source", choices: ["source", "bindle"], source: "system" }
                    ] } },
            open: { code: 0, output: null },
            check: { code: 0, output: null },
            update: { code: 0, output: null },
            "update-only": { code: 0, output: null },
            "apply-whats-new": { code: 0, output: "All 2 steps applied.\nUndo with `bindle undo`." },
            undo: { code: 0, output: "Undo snapshot 20260101-1\nDone." },
            "config-set": { code: 0, output: { ok: true, key: "news_days", value: "7", changed: true } },
            "config-reset": { code: 0, output: { ok: true, key: "space.boot_mb", value: "default", changed: true } }
        })

    FakeContext {
        id: fake
        commands: root.base
        events: ({
                check: [
                    { event: "step", text: "Checking for package updates" },
                    root.plan
                ],
                update: [
                    { event: "stage", index: 0, total: 3, id: "pacman", title: "Packages", summary: "2 packages" },
                    { event: "line", text: "downloading" },
                    { event: "stage", index: 1, total: 3, id: "flatpak", title: "Flatpak", summary: "1 update" },
                    { event: "result", id: "pacman", title: "Packages", outcome: "done", summary: "", why: "" },
                    { event: "result", id: "flatpak", title: "Flatpak", outcome: "failed", summary: "", why: "no network" },
                    { event: "done", failed: true }
                ],
                "update-only": [
                    { event: "stage", index: 0, total: 1, id: "flatpak", title: "Flatpak", summary: "1 update" },
                    { event: "result", id: "flatpak", title: "Flatpak", outcome: "done", summary: "", why: "" },
                    { event: "done", failed: false }
                ]
            })
        // Commands that have `confirm` in page.toml.
        confirmed: ["update", "update-only", "apply-whats-new", "undo"]
    }

    Loader {
        id: loader
        anchors.fill: parent
        source: Qt.resolvedUrl("../../uq/pages/bindle/Page.qml")
    }

    function shown(item, out) {
        out = out || [];
        if (typeof item.text === "string" && item.visible && item.text !== "")
            out.push(item.text);
        for (let i = 0; i < item.children.length; i++)
            shown(item.children[i], out);
        if (item.contentItem && item.contentItem !== item)
            shown(item.contentItem, out);
        return out;
    }

    function find(item, pred) {
        if (pred(item))
            return item;
        const kids = item.children;
        for (let i = 0; i < kids.length; i++) {
            const f = find(kids[i], pred);
            if (f)
                return f;
        }
        if (item.contentItem && item.contentItem !== item)
            return find(item.contentItem, pred);
        return null;
    }

    function button(text) {
        return find(loader.item, i => i.text === text && i.visible && i.clicked !== undefined);
    }

    TestCase {
        name: "BindlePage"
        when: windowShown

        function openTab(name) {
            const b = root.find(loader.item, i => i.objectName === "tab-" + name);
            verify(b !== null, "tab " + name);
            b.clicked();
        }

        function init() {
            fake.commands = root.base;
            fake.calls = [];
            fake.confirms = true;
            loader.source = "";
            loader.source = Qt.resolvedUrl("../../uq/pages/bindle/Page.qml");
            verify(loader.status === Loader.Ready, "the page loads");
            loader.item.ctx = fake.ctx;
        }

        function hasText(t) {
            tryVerify(() => root.shown(loader.item).indexOf(t) >= 0, 2000, t + " in " + JSON.stringify(root.shown(loader.item)));
        }

        function lacksText(t) {
            verify(root.shown(loader.item).indexOf(t) < 0, t + " should not show");
        }

        function ran(id) {
            return fake.calls.filter(c => (c[0] === "run" || c[0] === "stream") && c[1] === id);
        }

        function test_setup_shows_profile_and_missing_pieces() {
            hasText("Profile: desktop");
            hasText("Added to the profile: fstrim");
            hasText("Taken out of the profile: cups");
            hasText("2 steps would run again");
            hasText("Install 1 package");
            hasText("zram-generator");
            hasText("Run 1 setup command");
            hasText("Unlock KDE Wallet at login");
            // The raw steps stay out of sight until asked for, and a script shows as its first line.
            lacksText("• install package zram-generator");
            root.button("Show every step").clicked();
            hasText("• install package zram-generator");
            hasText("• sh -ec for f in /etc/pam.d/greetd; do …");
        }

        function test_pick_apps_opens_the_window() {
            hasText("Profile: desktop");
            root.button("Pick apps again").clicked();
            hasText("Bindle is opening in its own window.");
            compare(ran("open").length, 1);
        }

        function test_more_software_opens_bindaru_through_uq() {
            root.find(root, i => i.objectName === "open-bindaru").clicked();
            wait(100);
            compare(fake.calls.filter(c => c[0] === "openApp").map(c => c[1]), ["io.github.asafelobotomy.bindaru.desktop"]);
            hasText("bindaru is opening in its own window.");
        }

        function test_an_unreadable_status_is_an_error_card_not_a_blank_page() {
            fake.commands = Object.assign({}, root.base, { status: { error: "exit 1: boom" } });
            loader.item.loadSetup();
            hasText("Couldn't read this system's setup: exit 1: boom");
        }

        function test_updates_check_on_first_view_and_list_stages() {
            openTab("Updates");
            hasText("2 stages have updates");
            hasText("Packages");
            hasText("linux 6.1 → 6.2");
            hasText("the system upgrade has to run first");
            hasText("Arch news: 1 unread post");
            hasText("1 other stage is up to date.");
            compare(ran("check")[0], ["stream", "check", []]);
        }

        function test_update_now_asks_streams_and_looks_again() {
            openTab("Updates");
            hasText("2 stages have updates");
            const b = root.button("Update now");
            tryVerify(() => b.enabled);
            b.clicked();
            hasText("The update finished with failures.");
            hasText("Packages: updated");
            hasText("Flatpak: failed (no network)");
            compare(ran("update")[0], ["stream", "update", []]);
            // after an update it checks again
            tryVerify(() => ran("check").length === 2);
        }

        function test_a_declined_update_changes_nothing() {
            fake.confirms = false;
            openTab("Updates");
            hasText("2 stages have updates");
            const b = root.button("Update now");
            tryVerify(() => b.enabled);
            b.clicked();
            wait(100);
            lacksText("The update finished with failures.");
            tryVerify(() => b.enabled);
        }

        function test_one_stage_alone() {
            openTab("Updates");
            hasText("2 stages have updates");
            const b = root.find(loader.item, i => i.text === "Only this" && i.visible);
            verify(b !== null);
            b.clicked();
            tryVerify(() => ran("update-only").length === 1);
            compare(ran("update-only")[0], ["stream", "update-only", ["pacman"]]);
        }

        function test_whats_new_lists_items_and_applies_after_asking() {
            openTab("What's new");
            hasText("What's new in Bindle 0.1.12");
            hasText("Since Bindle 0.1.11");
            hasText("✓ zram swap");
            hasText("○ kyko");
            const b = root.button("Apply what's ticked…");
            tryVerify(() => b.enabled);
            b.clicked();
            hasText("All 2 steps applied.\nUndo with `bindle undo`.");
            compare(ran("apply-whats-new").length, 1);
        }

        function test_history_lists_snapshots_and_undoes_one() {
            openTab("History");
            hasText("2 snapshots");
            hasText("zram, fstrim");
            hasText("2026-01-01 · 3 files · 1 service · 2 packages");
            hasText("2025-12-01 · 1 file · undone".replace("1 file ·", "1 file · stopped early ·"));
            const b = root.button("Undo…");
            b.clicked();
            tryVerify(() => ran("undo").length === 1);
            compare(ran("undo")[0], ["run", "undo", ["20260101-1"]]);
            hasText("Undo snapshot 20260101-1\nDone.");
        }

        function test_preferences_show_sources_and_write_one_key() {
            openTab("Preferences");
            hasText("Arch news window (days)");
            hasText("Set for the whole system in /etc/bindle/upgrade.toml. Setting it here overrides it for you.");
            hasText("Your setting. The default is 200 MB.");
            hasText("kyko");
            const spin = root.find(loader.item, i => i.hasOwnProperty("value") && i.hasOwnProperty("from") && i.hasOwnProperty("editable") && i.value === 30);
            verify(spin !== null);
            spin.value = 7;
            spin.valueModified();
            tryVerify(() => ran("config-set").length === 1, 3000);
            compare(ran("config-set")[0], ["run", "config-set", ["news_days", "7"]]);
        }

        function test_reset_goes_back_to_the_default() {
            openTab("Preferences");
            hasText("Your setting. The default is 200 MB.");
            const b = root.find(loader.item, i => i.text === "Reset" && i.visible);
            verify(b !== null);
            b.clicked();
            tryVerify(() => ran("config-reset").length >= 1);
            compare(ran("config-reset")[0], ["run", "config-reset", ["space.boot_mb"]]);
        }

        function test_a_refused_preference_shows_the_reason() {
            fake.commands = Object.assign({}, root.base, { "config-set": { code: 1, error: "exit 1: battery_min_percent is from 0 to 100", output: null } });
            openTab("Preferences");
            hasText("Arch news window (days)");
            const spin = root.find(loader.item, i => i.hasOwnProperty("value") && i.hasOwnProperty("from") && i.hasOwnProperty("editable") && i.value === 30);
            spin.value = 7;
            spin.valueModified();
            hasText("exit 1: battery_min_percent is from 0 to 100");
        }
    }
}
