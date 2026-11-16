// Tests for the Bindle page in uQ Settings (uq/pages/bindle/): its logic.js, its manifest, and the
// way its QML uses the commands.   node --test 'tests/uq/*.test.mjs'
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { loadLogic } from "./load-logic.mjs";
import { parseToml } from "./toml-lite.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pageDir = join(here, "..", "..", "uq", "pages", "bindle");
const L = loadLogic(join(pageDir, "logic.js"));
const manifest = parseToml(readFileSync(join(pageDir, "page.toml"), "utf8"));
const qmlFiles = readdirSync(pageDir).filter((f) => f.endsWith(".qml"));
const read = (f) => readFileSync(join(pageDir, f), "utf8");
const command = (id) => manifest.command.find((c) => c.id === id);

// ---- manifest -------------------------------------------------------------------------------

test("the page uses uq_api 3 and every command it declares is used", () => {
  assert.equal(manifest.uq_api, 3);
  assert.equal(manifest.id, "bindle");
  const qml = qmlFiles.map(read).join("\n") + read("logic.js");
  const previews = new Set(manifest.command.map((c) => c.preview).filter(Boolean));
  for (const c of manifest.command) {
    if (previews.has(c.id)) continue; // run by uQ itself, before the confirmation
    assert.ok(qml.includes(`"${c.id}"`), `command ${c.id} is never used`);
  }
});

test("every command the QML runs is declared, and streams are ndjson", () => {
  const ids = new Set(manifest.command.map((c) => c.id));
  for (const f of qmlFiles) {
    for (const m of read(f).matchAll(/(?:\.write|\.stream|command\.run|command\.watch|\.update)\(\s*(?:"[^"]*",\s*)?"([a-z-]+)"/g)) {
      assert.ok(ids.has(m[1]) || m[1] === "check" || m[1] === "run", `${f} uses undeclared command ${m[1]}`);
    }
  }
  for (const id of ["check", "update", "update-only"]) assert.equal(command(id).output, "ndjson", id);
  for (const c of manifest.command) if (c.output === "ndjson") assert.ok(c.argv.includes("--ndjson"), `${c.id} has no --ndjson`);
  for (const c of manifest.command) assert.equal(c.argv[0], "/usr/lib/bindle/bindle", c.id);
});

test("a command with a preview confirms, and its preview is a read of the same page", () => {
  for (const c of manifest.command.filter((c) => c.preview)) {
    assert.ok(c.confirm, `${c.id} has a preview but no confirm`);
    const p = command(c.preview);
    assert.ok(p, `${c.id}: no command ${c.preview}`);
    assert.equal(p.output, "json");
    assert.ok(!p.confirm && !p.preview);
    assert.ok(p.argv.includes("--dry-run"), `${p.id} is not a dry run`);
    assert.deepEqual((p.args ?? []).map((a) => a.pattern), (c.args ?? []).map((a) => a.pattern), `${c.id}: preview takes other arguments`);
  }
  for (const id of ["update", "update-only", "apply-whats-new", "undo"]) assert.ok(command(id).preview, id);
});

test("tst_page.qml lists the commands that confirm", () => {
  const plan = manifest.command.filter((c) => c.confirm).map((c) => c.id).sort();
  const listed = /confirmed:\s*\[([^\]]*)\]/.exec(readFileSync(join(here, "tst_page.qml"), "utf8"));
  assert.ok(listed);
  assert.deepEqual([...listed[1].matchAll(/"([a-z-]+)"/g)].map((m) => m[1]).sort(), plan);
});

test("the page never forces anything, and the only unattended update is the one that confirms", () => {
  for (const c of manifest.command) {
    for (const flag of ["--force", "--ignore-news", "--on-battery", "--remove-orphans", "--firmware", "--sandbox"]) assert.ok(!c.argv.includes(flag), `${c.id} passes ${flag}`);
  }
  for (const c of manifest.command.filter((c) => c.argv.includes("--ndjson") && !c.argv.includes("--check"))) assert.ok(c.confirm && c.preview, `${c.id} changes the system without asking`);
});

test("argument patterns are the same in logic.js and page.toml", () => {
  let seen = 0;
  for (const c of manifest.command) {
    for (const a of c.args ?? []) {
      seen++;
      assert.equal(a.pattern, L.PATTERNS[c.id]?.[a.name], `${c.id}.${a.name}`);
    }
    assert.equal((c.args ?? []).length, Object.keys(L.PATTERNS[c.id] ?? {}).length, `${c.id}: argument count`);
  }
  assert.equal(seen, 7);
});

test("the stage names the page accepts are the stages bindleup has", () => {
  const rust = readFileSync(join(here, "..", "..", "crates", "bindle-upgrade", "src", "stage.rs"), "utf8");
  const ids = [...rust.matchAll(/Stage::\w+ => "([a-z]+)",/g)].map((m) => m[1]);
  assert.deepEqual([...L.STAGES].sort(), [...new Set(ids)].sort());
});

const HOSTILE = ["-rf", "--yes", "a\nb", "", "x".repeat(200), "../x", "$(id)", "-", "a b", "A\u0000"];
test("hostile arguments are refused by logic.js exactly as uQ would", () => {
  const uqAccepts = (pattern, v) => v !== "" && v[0] !== "-" && Buffer.byteLength(v) <= 4096 && new RegExp(pattern).test(v);
  const good = { stage: "pacman", id: "20260101-1", key: "news_days", value: "7" };
  for (const [id, args] of Object.entries(L.PATTERNS)) {
    const names = Object.keys(args);
    for (const [i, name] of names.entries()) {
      for (const bad of HOSTILE) {
        const given = names.map((n, j) => (j === i ? bad : good[n]));
        assert.equal(L.argsProblem(id, given) === "", uqAccepts(args[name], bad), `${id}.${name} ${JSON.stringify(bad.slice(0, 12))}`);
      }
    }
  }
  assert.equal(L.argsProblem("update-only", ["aur"]), "");
  assert.notEqual(L.argsProblem("update-only", ["kernel"]), "");
  assert.equal(L.argsProblem("config-set", ["releases.apps.kyko", "bindle"]), "");
  assert.notEqual(L.argsProblem("config-set", ["releases.apps.a.b", "bindle"]), "");
  assert.equal(L.argsProblem("config-set", ["space.boot_mb", "300"]), "");
  assert.notEqual(L.argsProblem("config-set", ["colour", "red"]), "");
});

// ---- the page's text ------------------------------------------------------------------------

const BANNED = /D-?Bus|polkit|JSON|systemctl|sqlite|ndjson|\borg\.[a-z0-9_]+\.[A-Za-z0-9_.]+/i;
test("the page's words are plain", () => {
  const found = [];
  const literals = (line) => [...line.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1]);
  for (const f of qmlFiles) for (const line of read(f).split("\n")) if (/\btext:|title:|detail:|help:|label:|placeholderText:/.test(line)) found.push(...literals(line));
  for (const line of read("logic.js").split("\n")) {
    if (/^\s*\/\//.test(line) || /PATTERN|pattern|RegExp|\.test\(|case |\.event|ev\./.test(line)) continue;
    found.push(...literals(line).filter((s) => /^[A-Z].* /.test(s) || /…/.test(s)));
  }
  found.push(manifest.name, ...manifest.setting.map((s) => s.label), ...manifest.command.filter((c) => c.confirm).map((c) => c.confirm));
  assert.ok(found.length > 40, `only ${found.length} strings`);
  for (const s of found) assert.ok(!BANNED.test(s), `jargon in: ${s}`);
});

// ---- reading what bindle prints -------------------------------------------------------------

test("setupView reads the status and refuses what it can't trust", () => {
  const v = L.setupView({ code: 0, output: { selection: "/x", profile: "desktop", with: ["a", 3, "b"], without: [], last_applied: 1700000000, missing: ["install x"], error: null } });
  assert.equal(v.has, true);
  assert.deepEqual(v.with, ["a", "b"]);
  assert.deepEqual(v.missing, ["install x"]);
  assert.equal(L.setupView({ code: 0, output: { selection: null, profile: null, with: [], without: [], missing: [] } }).has, false);
  assert.match(L.setupView({ error: "boom" }).problem, /^Couldn't read this system's setup: boom$/);
  assert.match(L.setupView({ code: 0, output: "text" }).problem, /doesn't understand/);
  assert.equal(L.setupView({ error: "Cancelled", cancelled: true }), null);
  assert.equal(L.setupView({ error: "already running" }), null);
  assert.equal(L.setupView({ code: 0, output: { selection: "/x", missing: Array.from({ length: 900 }, (_, i) => `s${i}`) } }).missing.length, 300);
});

test("progress follows the events of a check and of a run", () => {
  let s = L.startProgress("check");
  assert.equal(L.progressLine(s), "Looking at the system…");
  s = L.reduce(s, { event: "step", text: "Checking for package updates" });
  assert.equal(L.progressLine(s), "Checking for package updates…");
  assert.equal(L.fraction(s), -1);
  s = L.reduce(s, { event: "plan", work: 1, stages: [{ id: "pacman", title: "Packages", state: "run", summary: "2 packages", details: ["a", "b"], needs_root: true }, { id: "news", title: "Arch news", state: "bogus" }, null] });
  assert.equal(L.progressLine(s), "1 stage has work to do.");
  const g = L.planGroups(s.plan);
  assert.deepEqual([g.work.length, g.notes.length, g.held.length, g.current], [1, 1, 0, 0]);
  assert.equal(g.work[0].needsRoot, true);

  let r = L.startProgress("run");
  assert.equal(L.progressLine(r), "Starting…");
  r = L.reduce(r, { event: "stage", index: 1, total: 4, id: "pacman", title: "Packages", summary: "2 packages" });
  assert.equal(L.progressLine(r), "Step 2 of 4: Packages (2 packages)");
  assert.equal(L.fraction(r), 0.25);
  r = L.reduce(r, { event: "result", id: "pacman", title: "Packages", outcome: "failed", why: "nope" });
  r = L.reduce(r, { event: "relaunch" });
  assert.equal(r.results.length, 0);
  assert.equal(L.progressLine(r), "Bindle updated itself and goes on…");
  r = L.reduce(r, { event: "done", failed: true });
  assert.equal(L.progressLine(r), "The update finished with failures.");
  assert.equal(L.reduce(r, "junk"), r);
  assert.equal(L.outcomeWord("held"), "held back");
});

test("planGroups caps long detail lists and long text", () => {
  const s = L.reduce(L.startProgress("check"), { event: "plan", work: 1, stages: [{ id: "pacman", title: "T".repeat(500), state: "run", summary: "x", details: Array.from({ length: 100 }, (_, i) => `p${i}`) }] });
  const st = s.plan.stages[0];
  assert.equal(st.details.length, 40);
  assert.equal(st.more, 60);
  assert.ok(st.title.length <= 200);
});

test("streamProblem says something only when the events did not", () => {
  const done = L.reduce(L.startProgress("run"), { event: "done", failed: false });
  assert.equal(L.streamProblem(done, { error: "timed out" }), "");
  assert.equal(L.streamProblem(L.startProgress("run"), { error: "timed out" }), "timed out");
  assert.equal(L.streamProblem(L.startProgress("run"), { error: "Cancelled", cancelled: true }), "");
});

test("whatsNewView groups the items in the window's order", () => {
  const v = L.whatsNewView({ code: 0, output: { version: "0.1.12", since: "0.1.11", recorded: false, items: [
    { id: "b", name: "B", kind: "retired", ticked: false, changes: [] },
    { id: "a", name: "A", kind: "new", ticked: true, why: "w", changes: [{ text: "+ x" }, { nope: 1 }] },
    { id: "c", name: "C", kind: "weird", ticked: true },
    { name: "no id" }] } });
  assert.deepEqual(v.groups.map((g) => g.kind), ["new", "updated", "retired"]);
  assert.equal(v.ticked, 2);
  assert.equal(v.recorded, false);
  assert.deepEqual(v.groups[0].items[0].changes, ["+ x"]);
  assert.match(L.whatsNewView({ error: "x" }).problem, /^Couldn't read what's new/);
});

test("historyView keeps valid ids and words each snapshot", () => {
  const v = L.historyView({ code: 0, output: { snapshots: [
    { id: "20260101-1", created: "c", label: "a, b, c, d, e, f", files: 3, units: 1, packages: 0, undone: false, complete: false },
    { id: "../x", label: "bad" }, { id: "-rf" }] } });
  assert.equal(v.snapshots.length, 1);
  assert.equal(v.snapshots[0].label, "a, b, c, +3 more");
  assert.equal(L.snapshotLine(v.snapshots[0]), "3 files · 1 service · stopped early");
  assert.equal(L.historyView({ code: 0, output: { snapshots: [] } }).snapshots.length, 0);
});

test("configView groups keys, keeps only known keys and says where a value comes from", () => {
  const key = (o) => ({ key: "news_days", group: "Checks", label: "L", help: "h", kind: "int", value: 30, default: 30, min: 0, max: 99, unit: "days", source: "default", ...o });
  const v = L.configView({ code: 0, output: { update_notify: true, path: "/p", apps: [{ app: "kyko", value: "bindle", source: "user" }, { app: "a b", value: "bindle" }, { app: "x", value: "aur" }], keys: [key({}), key({ key: "space.boot_mb", group: "Free space", source: "system", value: 500, default: 200, unit: "MB" }), key({ key: "evil key" }), key({ key: "releases.follow", group: "Apps", kind: "choice", value: "source", default: "source", choices: ["source", "bindle"] })] } });
  assert.deepEqual(v.groups.map((g) => g.title), ["Checks", "Free space", "Apps"]);
  assert.equal(v.notify, true);
  assert.deepEqual(v.apps.map((a) => a.app), ["kyko"]);
  const boot = v.groups[1].keys[0];
  assert.match(L.sourceNote(boot), /whole system/);
  assert.equal(L.sourceNote({ ...boot, source: "user" }), "Your setting. The default is 200 MB.");
  assert.equal(L.sourceNote({ ...boot, source: "default" }), "");
  assert.match(L.configView({ error: "boom" }).problem, /^Couldn't read the update preferences: boom$/);
});

test("outcomes: a refusal shows its reason and the command's last lines", () => {
  assert.equal(L.textOutcome({ error: "Cancelled", cancelled: true }, "x"), null);
  assert.deepEqual(L.textOutcome({ code: 0, output: null }, "Done"), { message: "Done", failed: false });
  assert.equal(L.textOutcome({ code: 0, output: null }, ""), null);
  const tail = L.textOutcome({ code: 0, output: "1\n2\n3\n4\n5\n6" }, "@output");
  assert.equal(tail.message, "3\n4\n5\n6");
  const bad = L.textOutcome({ error: "exit 1: nope", output: "x\ny" }, "");
  assert.equal(bad.failed, true);
  assert.match(bad.message, /^exit 1: nope\nx\ny$/);
});
