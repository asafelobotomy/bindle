.pragma library
// The Bindle page's pure logic: no Qt calls, so Node can test it (tests/uq/logic.test.mjs).
// Everything `bindle` prints is untrusted: it is read defensively, a wrong type becomes an empty or
// "couldn't read" result, lists are capped and strings are cut. The words are the ones of the
// bindle-gui window and `bindleup`, so all three say the same thing.

var NAME_LIMIT = 200;
var MESSAGE_LIMIT = 400;
var LIST_LIMIT = 300;
var DETAIL_LIMIT = 40;

var KEY_PATTERN = "^(news_days|mirror_max_age_days|firmware_refresh_days|space\\.(root|boot|efi)_mb|battery_min_percent|battery_max_packages|releases\\.follow|releases\\.apps\\.[A-Za-z0-9_-]{1,64}|update_notify|bar_icon)$";
var ID_PATTERN = "^[0-9A-Za-z_-]{1,64}$";
var STAGES = ["bindle", "news", "preflight", "snapshot", "mirrors", "pacman", "aur", "releases", "flatpak", "firmware", "rebuild", "uq", "pacnew", "orphans", "services", "setup", "reboot"];
var STAGE_PATTERN = "^(" + STAGES.join("|") + ")$";

/// The argument patterns of page.toml's [[command]] tables: command id -> argument -> pattern.
/// tests/uq/logic.test.mjs checks that they are the same strings.
var PATTERNS = {
    "preview-update-only": { "stage": STAGE_PATTERN },
    "update-only": { "stage": STAGE_PATTERN },
    "preview-undo": { "id": ID_PATTERN },
    "undo": { "id": ID_PATTERN },
    "config-set": { "key": KEY_PATTERN, "value": "^[a-z0-9]{1,16}$" },
    "config-reset": { "key": KEY_PATTERN }
};

function cut(text, limit) {
    const s = String(text);
    return s.length > limit ? s.slice(0, limit - 1) + "…" : s;
}

function plural(n, one, many) {
    return n + " " + (n === 1 ? one : many);
}

function str(v, fallback, limit) {
    return typeof v === "string" && v !== "" ? cut(v, limit || NAME_LIMIT) : fallback;
}

function num(v, fallback) {
    return typeof v === "number" && isFinite(v) ? v : fallback;
}

function argsProblem(command, args) {
    const patterns = PATTERNS[command];
    if (!patterns)
        return "";
    const names = Object.keys(patterns);
    if (args.length !== names.length)
        return "Wrong number of arguments for " + command;
    for (let i = 0; i < names.length; i++) {
        if (typeof args[i] !== "string" || args[i][0] === "-" || !new RegExp(patterns[names[i]]).test(args[i]))
            return "That " + names[i] + " can't be used here";
    }
    return "";
}

// ---- results

function isCancelled(result) {
    return !!result && result.cancelled === true;
}

/// A command's answer as {ok, error, output}: bindle's own error line when it refused, else the
/// transport error.
function envelope(result) {
    if (!result)
        return { ok: false, error: "bindle gave no answer", output: null };
    const out = result.output !== undefined ? result.output : null;
    if (result.error !== undefined)
        return { ok: false, error: cut(result.error, MESSAGE_LIMIT), output: out };
    return { ok: true, error: "", output: out };
}

/// What an action said, for the row under it: {message, failed}, or null when nothing needs saying.
function textOutcome(result, okText) {
    if (isCancelled(result))
        return null;
    const env = envelope(result);
    if (!env.ok) {
        const detail = typeof env.output === "string" ? env.output.trim().split("\n").slice(-3).join("\n") : "";
        return { message: detail !== "" ? env.error + "\n" + cut(detail, MESSAGE_LIMIT) : env.error, failed: true };
    }
    if (okText === "@output") {
        // What the command said last: the packages undo leaves installed, the log of an apply.
        const tail = typeof env.output === "string" ? env.output.trim().split("\n").slice(-4).join("\n") : "";
        return tail !== "" ? { message: cut(tail, MESSAGE_LIMIT), failed: false } : { message: "Done.", failed: false };
    }
    return okText ? { message: okText, failed: false } : null;
}

function withNote(notes, key, note) {
    const next = Object.assign({}, notes);
    if (note === null || note === undefined)
        delete next[key];
    else
        next[key] = note;
    return next;
}

function problemOf(result, what) {
    if (isCancelled(result) || (result && result.error === "already running"))
        return null;
    const env = envelope(result);
    return env.ok ? "" : "Couldn't read " + what + ": " + env.error;
}

// ---- setup

function when(secs) {
    if (!secs || typeof secs !== "number")
        return "";
    return new Date(secs * 1000).toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" });
}

/// A step that runs a script reads as its first line; the rest is the script's body.
function firstLine(text) {
    const lines = text.split("\n");
    return lines.length > 1 ? lines[0].replace(/[\s;\\]+$/, "") + " …" : text;
}

/// What the missing steps amount to: [{kind, label, detail}], `detail` the first names of its items.
function groupsOf(v) {
    if (!Array.isArray(v))
        return [];
    return v.slice(0, 12).filter(g => g && typeof g === "object" && typeof g.label === "string").map(g => {
        const items = Array.isArray(g.items) ? g.items.filter(x => typeof x === "string") : [];
        const shown = items.slice(0, 8).join(", ");
        return { kind: str(g.kind, "other"), label: cut(g.label, 80), detail: items.length > 8 ? shown + " and " + (items.length - 8) + " more" : shown };
    });
}

/// `bindle status --json` as {has, profile, lastApplied, with, without, missing, groups, error, problem}.
function setupView(result) {
    const p = problemOf(result, "this system's setup");
    if (p === null)
        return null;
    const none = { has: false, profile: "", lastApplied: 0, with: [], without: [], missing: [], groups: [], error: "", problem: p };
    const o = result && result.output;
    if (p !== "" || !o || typeof o !== "object")
        return p !== "" ? none : Object.assign(none, { problem: "bindle gave a status this page doesn't understand." });
    const list = v => (Array.isArray(v) ? v.slice(0, LIST_LIMIT).filter(x => typeof x === "string").map(x => cut(x, MESSAGE_LIMIT)) : []);
    return {
        has: !!o.selection,
        profile: str(o.profile, ""),
        lastApplied: num(o.last_applied, 0),
        with: list(o.with),
        without: list(o.without),
        missing: list(o.missing).map(firstLine),
        groups: groupsOf(o.groups),
        error: str(o.error, "", MESSAGE_LIMIT),
        problem: ""
    };
}

// ---- updates: the events of `upgrade --check --ndjson` and `upgrade --ndjson`

/// {kind, steps, stage, results, plan, done, failed, relaunched}. `kind` is "check" or "run".
function startProgress(kind) {
    return { kind: kind, step: "", stage: null, results: [], plan: null, done: false, failed: false, relaunched: false };
}

function stageView(s) {
    return {
        id: str(s.id, ""),
        title: str(s.title, str(s.id, "")),
        state: ["run", "note", "current", "skipped", "held"].indexOf(s.state) >= 0 ? s.state : "note",
        summary: str(s.summary, "", MESSAGE_LIMIT),
        why: str(s.why, "", MESSAGE_LIMIT),
        details: Array.isArray(s.details) ? s.details.slice(0, DETAIL_LIMIT).filter(d => typeof d === "string").map(d => cut(d.trim(), MESSAGE_LIMIT)) : [],
        more: Array.isArray(s.details) ? Math.max(0, s.details.length - DETAIL_LIMIT) : 0,
        needsRoot: s.needs_root === true
    };
}

function outcomeWord(o) {
    return { done: "updated", current: "already current", note: "see the note", skipped: "skipped", held: "held back", failed: "failed" }[o] || String(o);
}

function reduce(state, ev) {
    if (!ev || typeof ev !== "object")
        return state;
    const next = Object.assign({}, state, { results: state.results.slice() });
    switch (ev.event) {
    case "step":
        next.step = str(ev.text, "");
        break;
    case "plan":
        if (Array.isArray(ev.stages))
            next.plan = { stages: ev.stages.slice(0, 40).filter(s => s && typeof s === "object").map(stageView), work: num(ev.work, 0) };
        break;
    case "stage":
        next.stage = { index: num(ev.index, 0), total: Math.max(1, num(ev.total, 1)), title: str(ev.title, str(ev.id, "")), summary: str(ev.summary, "", MESSAGE_LIMIT) };
        next.step = "";
        break;
    case "result":
        if (next.results.length < 40)
            next.results.push({ id: str(ev.id, ""), title: str(ev.title, str(ev.id, "")), outcome: str(ev.outcome, "note", 20), summary: str(ev.summary, "", MESSAGE_LIMIT), why: str(ev.why, "", MESSAGE_LIMIT) });
        break;
    case "relaunch":
        next.relaunched = true;
        next.results = [];
        next.stage = null;
        break;
    case "done":
        next.done = true;
        next.failed = ev.failed === true;
        break;
    }
    return next;
}

/// 0..1 for a run's bar, -1 (busy) for a check, which has no known length.
function fraction(state) {
    if (state.kind !== "run" || state.stage === null)
        return -1;
    return Math.min(1, Math.max(0, state.stage.index / state.stage.total));
}

function progressLine(state) {
    if (state.kind === "check") {
        if (state.plan !== null)
            return state.plan.work === 0 ? "Everything is up to date." : plural(state.plan.work, "stage has", "stages have") + " work to do.";
        return state.step !== "" ? state.step + "…" : "Looking at the system…";
    }
    if (state.done)
        return state.failed ? "The update finished with failures." : "The update finished.";
    if (state.stage === null)
        return state.relaunched ? "Bindle updated itself and goes on…" : "Starting…";
    return "Step " + (state.stage.index + 1) + " of " + state.stage.total + ": " + state.stage.title + (state.stage.summary !== "" ? " (" + state.stage.summary + ")" : "");
}

/// What a finished stream says when the events did not explain how it ended.
function streamProblem(state, result) {
    if (isCancelled(result) || state.done || (state.kind === "check" && state.plan !== null))
        return "";
    if (result && result.error !== undefined)
        return cut(result.error, MESSAGE_LIMIT);
    return "";
}

/// The plan split for the cards: stages with work, notes, held back, and the count that is current.
function planGroups(plan) {
    const out = { work: [], notes: [], held: [], current: 0 };
    if (!plan)
        return out;
    for (const s of plan.stages) {
        if (s.state === "run")
            out.work.push(s);
        else if (s.state === "note")
            out.notes.push(s);
        else if (s.state === "held")
            out.held.push(s);
        else if (s.state === "current")
            out.current++;
    }
    return out;
}

// ---- what's new

var KIND_TITLES = {
    "new": "New for this computer",
    "updated": "Changed since it was set up",
    "optional": "New in this version (optional)",
    "retired": "No longer in Bindle (clean-up)",
    "removed": "You took these out of your setup (clean-up)"
};
var KIND_ORDER = ["new", "updated", "optional", "retired", "removed"];

/// `whats-new --json` as {version, since, recorded, groups: [{title, items}], ticked, problem}.
function whatsNewView(result) {
    const p = problemOf(result, "what's new");
    if (p === null)
        return null;
    const empty = { version: "", since: "", recorded: true, groups: [], ticked: 0, problem: p };
    const o = result && result.output;
    if (p !== "" || !o || typeof o !== "object" || !Array.isArray(o.items))
        return p !== "" ? empty : Object.assign(empty, { problem: "bindle gave a list this page doesn't understand." });
    const items = o.items.slice(0, LIST_LIMIT).filter(i => i && typeof i.id === "string").map(i => ({
        id: cut(i.id, NAME_LIMIT),
        name: str(i.name, i.id),
        kind: KIND_ORDER.indexOf(i.kind) >= 0 ? i.kind : "updated",
        why: str(i.why, "", MESSAGE_LIMIT),
        ticked: i.ticked === true,
        changes: Array.isArray(i.changes) ? i.changes.slice(0, 20).filter(c => c && typeof c.text === "string").map(c => cut(c.text, MESSAGE_LIMIT)) : []
    }));
    const groups = KIND_ORDER.map(k => ({ kind: k, title: KIND_TITLES[k], items: items.filter(i => i.kind === k) })).filter(g => g.items.length > 0);
    return { version: str(o.version, ""), since: str(o.since, ""), recorded: o.recorded !== false, groups: groups, ticked: items.filter(i => i.ticked).length, problem: "" };
}

// ---- history

/// `undo --list --json` as {snapshots: [{id, created, label, files, units, packages, undone, complete}], problem}.
function historyView(result) {
    const p = problemOf(result, "Bindle's history");
    if (p === null)
        return null;
    const o = result && result.output;
    if (p !== "" || !o || typeof o !== "object" || !Array.isArray(o.snapshots))
        return { snapshots: [], problem: p !== "" ? p : "bindle gave a history this page doesn't understand." };
    const snaps = o.snapshots.slice(0, 100).filter(s => s && typeof s.id === "string" && s.id[0] !== "-" && new RegExp(ID_PATTERN).test(s.id)).map(s => ({
        id: s.id,
        created: str(s.created, ""),
        label: labelOf(str(s.label, "")),
        files: num(s.files, 0),
        units: num(s.units, 0),
        packages: num(s.packages, 0),
        undone: s.undone === true,
        complete: s.complete !== false
    }));
    return { snapshots: snaps, problem: "" };
}

function labelOf(label) {
    const mods = label.split(", ").filter(m => m !== "");
    return mods.length > 4 ? mods.slice(0, 3).join(", ") + ", +" + (mods.length - 3) + " more" : label;
}

function snapshotLine(s) {
    const parts = [plural(s.files, "file", "files")];
    if (s.units > 0)
        parts.push(plural(s.units, "service", "services"));
    if (s.packages > 0)
        parts.push(plural(s.packages, "package", "packages"));
    return parts.join(" · ") + (s.complete ? "" : " · stopped early");
}

// ---- preferences

/// `config show --json` as {groups: [{title, keys}], apps, notify, path, problem}.
function configView(result) {
    const p = problemOf(result, "the update preferences");
    if (p === null)
        return null;
    const o = result && result.output;
    if (p !== "" || !o || typeof o !== "object" || !Array.isArray(o.keys))
        return { groups: [], apps: [], notify: false, path: "", problem: p !== "" ? p : "bindle gave preferences this page doesn't understand." };
    const keys = o.keys.slice(0, 40).filter(k => k && typeof k.key === "string" && k.key[0] !== "-" && new RegExp(KEY_PATTERN).test(k.key)).map(k => ({
        key: k.key,
        group: str(k.group, "Other"),
        label: str(k.label, k.key),
        help: str(k.help, "", MESSAGE_LIMIT),
        kind: k.kind === "choice" ? "choice" : "int",
        value: k.kind === "choice" ? str(k.value, "") : num(k.value, 0),
        def: k.kind === "choice" ? str(k.default, "") : num(k.default, 0),
        min: num(k.min, 0),
        max: num(k.max, 100000),
        unit: str(k.unit, "", 20),
        choices: Array.isArray(k.choices) ? k.choices.filter(c => typeof c === "string").slice(0, 8) : [],
        labels: choiceLabels(k),
        source: ["user", "system"].indexOf(k.source) >= 0 ? k.source : "default"
    }));
    const titles = [];
    for (const k of keys)
        if (titles.indexOf(k.group) < 0)
            titles.push(k.group);
    const apps = Array.isArray(o.apps) ? o.apps.slice(0, 100).filter(a => a && typeof a.app === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(a.app) && (a.value === "source" || a.value === "bindle")).map(a => ({ app: a.app, value: a.value, source: a.source === "user" ? "user" : "system" })) : [];
    return { groups: titles.map(t => ({ title: t, keys: keys.filter(k => k.group === t) })), apps: apps, notify: o.update_notify === true, path: str(o.path, ""), problem: "" };
}

/// What the page shows for each of a choice's values: bindle's own names when it gives one for each.
function choiceLabels(k) {
    const choices = Array.isArray(k.choices) ? k.choices.filter(c => typeof c === "string").slice(0, 8) : [];
    const labels = Array.isArray(k.choice_labels) ? k.choice_labels.slice(0, 8) : [];
    if (labels.length === choices.length && labels.every(l => typeof l === "string" && l !== ""))
        return labels.map(l => str(l, ""));
    return choices;
}

function sourceNote(k) {
    if (k.source === "system")
        return "Set for the whole system in /etc/bindle/upgrade.toml. Setting it here overrides it for you.";
    if (k.source === "user" && k.kind === "choice") {
        const i = k.choices && k.choices.indexOf ? k.choices.indexOf(k.def) : -1;
        const name = i >= 0 && k.labels && typeof k.labels[i] === "string" ? k.labels[i] : k.def;
        return "Your setting. The default is “" + name + "”.";
    }
    if (k.source === "user")
        return "Your setting. The default is " + k.def + (k.unit !== "" ? " " + k.unit : "") + ".";
    return "";
}

var FOLLOW_NAMES = ["Only copies Bindle installed", "Bindle's release replaces any copy"];
var FOLLOW_KEYS = ["source", "bindle"];

function indexOfKey(keys, key, fallback) {
    const i = keys.indexOf(key);
    return i >= 0 ? i : fallback;
}
