# What's new: upgrading from an older Bindle

A newer Bindle can bring new modules, change what a module installs or
writes, or retire a module. "What's new" shows each of these for *this*
computer, with the exact changes, and applies only what the user picks,
through the usual review and snapshot (so `bindle undo` reverts it).

```bash
bindle whats-new            # pick in the terminal, then review and apply
bindle whats-new --list     # just show it
bindle whats-new --json     # for scripts (bindleupgrade's report uses the same diff)
bindle whats-new --yes      # apply what is ticked by default (new and changed)
bindle whats-new --all      # include the items you hid
bindo whats-new
bindle-gui --whats-new      # the window's page
```

It also opens by itself when `bindle` (the terminal wizard) or `bindle-gui`
starts and there is something in it, until you are done with this version:
"Not now" (q) stores nothing, so it asks again next time; "Skip this version"
(s), or applying, stops it opening by itself until a newer Bindle. "Hide"
(h) removes one item for good, remembered by its content, so a later Bindle
that changes it again shows it again (`~/.local/state/bindle/whats-new.json`;
`--all` shows hidden items). after bindleupgrade updates Bindle
itself (asked on a terminal, otherwise a notification with Review); and from
bindleupgrade's "Bindle setup" stage and the update window's What's new button.

## How it works

### 1. Every apply records what it did

Each apply's snapshot (`/var/lib/bindle/snapshots/<id>/manifest.json`) holds
the Bindle `version`, the `record` (per module, what it puts on this system,
resolved for this machine: packages with a builtin's computed ones, AUR
packages, release pins, files with their content, units, groups, the commands
whose conditions hold, pacman.conf options; `Plan::contents`, collected by
`plan::resolve`) and `forget` (modules the apply retired).

The record also goes to a **ledger** that outlives the snapshots
(`bindle-run/src/record.rs`): append-only `(apply, version, module, content)`
entries, where an empty content means the module was retired.

- The root helper adds an apply's entries when it completes
  (`/var/lib/bindle/record.json`) and drops them when `bindle undo` restores
  it, so the record follows undo, and deleting snapshots doesn't lose it.
- An apply with nothing to do as root takes no snapshot; it adds its entries
  to the user's ledger (`~/.local/state/bindle/record.json`).
- The record is both ledgers in apply order (a later entry wins), plus
  snapshots from before the ledger (complete, not undone). Old snapshots that
  name modules without content mark them as set up before records: those are
  compared with what is on the system, and the page says so. With no trace at
  all, a module that is partly installed counts the same way.

### 2. Diff, not migrations

`bindle_core::whatsnew::diff` (pure, unit-tested from fixtures) compares the
record with the saved selection (`~/.config/bindle/selection.toml`, else
`/etc/bindle/selection.toml`) planned by this Bindle:

| Group | What | Ticked |
|---|---|---|
| New for this computer | modules the selection now brings that no apply covered and that aren't in place | yes |
| Changed since it was set up | applied modules whose content changed: `+ installs gutenprint`, `~ changes /etc/…` with a line diff, units, groups, commands, release pins | yes |
| No longer in Bindle (clean-up) | applied modules that left the catalog or the profile, that a selected module replaces, or that no longer fit this computer, with what the clean-up does | no |
| You took these out of your setup (clean-up) | applied modules the user excluded from their selection; what they set up is still here | no |
| New in this version (optional) | modules `catalog/changes.toml` lists for versions after the last apply, that fit this computer and aren't selected | no |

`catalog/changes.toml` covers what a diff can't infer, validated against the
catalog by the tests:

- `[[renamed]] from, to, version, why`: what was applied under the old id
  counts as the new one (neither new nor retired).
- `[[retired]] id, version, why, replaced_by`: the reason shown for a module
  that left the catalog.
- `[[added]] version, modules, why`: optional modules worth showing to people
  upgrading to that version.

No module has been renamed or removed so far (checked against every tag), so
only `added` has entries.

### 3. Review, then apply only what's picked

Inside a picked item, a single package, AUR package, file or service can be
left out (its own tick; `Change::key`). Its step is dropped
(`whatsnew::pick::filter_steps`), and the module's recorded content keeps the
old value for it (`pick::revert`), so a later What's new shows it again. The
rest of a module (groups, commands, pacman.conf, release pins) goes with it.

The picks become an *exact* selection (`Selection::exact`: just those modules
and what they require, no profile defaults), planned and applied like any
other: the Review page (or the terminal's review) lists every step, a snapshot
is taken first, and the new apply records the picked modules. A picked retired
module's clean-up removes its files (only ones that still start with
`# Managed by Bindle`) and turns off its units that no selected module uses;
packages stay, with the `pacman -Rns` command shown. Afterwards the saved
selection follows the picks: optional modules join it, cleaned-up ones leave
it. Unpicked items stay as they are and show again next time.

## Code

| Where | What |
|---|---|
| `bindle-core/src/plan/content.rs` | per-module content, collected by `plan::resolve` |
| `bindle-core/src/whatsnew.rs` (+ `changes.rs`, `linediff.rs`, `tests.rs`) | the pure diff, the table, line diffs |
| `bindle-run/src/whatsnew.rs` | the record from snapshots, the saved selection, the plan for the picks, the seen version |
| `bindle-cli/src/whatsnew_cmd.rs`, `tui/whatsnew.rs` | `bindle whats-new`, the terminal page, the first-run hook |
| `bindle-gui/src/model/whatsnew.rs`, `qml/WhatsNewPage.qml` | the window's page (steps: What's new → Review → Apply) |
| `bindle-upgrade/src/setup.rs` | bindleupgrade's "Bindle setup" stage reports What's new |

## Choices made

- **Where the record lives**: the snapshot of each apply, and a ledger that
  outlives it (system and user), undo-aware.
- **Granularity**: whole items, plus single packages, AUR packages, files and
  services within one; other parts follow the module.
- **Removed by you vs retired**: their own group; both clean-ups unticked.
- **When it opens by itself**: until applied or skipped for this version;
  "Not now" asks again; hidden items stay hidden until they change.
