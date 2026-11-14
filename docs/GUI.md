# GUI

A desktop window for the wizard, replacing the terminal at first login on
graphical sessions. The terminal wizard stays for TTYs, SSH and as a fallback.

uQ (`asafelobotomy/uq`) is Bindle's main desktop. Its design spec makes Bindle
uQ's first-run installer: Bindle installs the `uq` module, hands off to uQ's
Welcome, and adds a "Re-run setup" page to uQ Settings. The GUI
follows uQ's app conventions so it can fill that role.

## Decisions

| Question | Choice | Why |
|---|---|---|
| Toolkit | Rust + cxx-qt 0.10 + Kirigami (QML) | uQ's app stack, and what hauler, appOFholding and noti-fi already use; the same QML can be loaded as a uq-settings page |
| Scope | Full wizard and apply | Choosing, reviewing and applying all happen in the window, with live progress |
| Binary | Separate `bindle-gui`, linked against the system's Qt 6 | The CLI stays a static musl binary for the live ISO |
| Root | `pkexec bindle helper`, with a polkit action | The same JSON helper the CLI uses; the policy gives the password dialog a clear message |
| Theme | uQ tokens when present, else the desktop's Qt theme | See below |

## Theming

- **On uQ**: read `~/.local/state/uq/theme/tokens.json` and watch it for
  changes, as uq-shell does. The tokens (`bg`, `surface`, `surface_alt`,
  `overlay`, `text`, `text_muted`, `accent`, `accent_text`, `border`,
  `success`, `warning`, `error`, `radius`, fonts) map onto `Kirigami.Theme`
  colours and units, so Bindle re-themes live when the user changes theme or
  variant, whether or not they opted in to uQ's Qt app theming.
- **Elsewhere**: Kirigami follows the desktop's Qt theme (Breeze on Plasma,
  qt6ct where configured, the default Qt style otherwise).
- No colours in view files: every component reads from one `Theme` singleton.

## Screens

Welcome → Profile → Choose → Summary → Review → Apply, shown in a step bar
that goes back to any screen already visited.

- **Welcome** (`WelcomePage.qml`): "Already working" (what the installer set
  up) beside "Bindle suggests" (what is picked for this computer, each a way
  into its category). The installer's full summary folds under "System
  details".
- **Profile** (`ProfilePage.qml`): a base (Minimal, Desktop) and any add-ons
  on top (Gaming, Developer: profiles with `addon = true` in
  `catalog/profiles.toml`), each naming what it brings. Changing the base
  after editing picks asks first.
- **Choose** (`ChoosePage.qml`): one category at a time, its groups of
  modules in a grid, list or compact view, with search, filters (All,
  Selected, Recommended; hide what isn't available, only safe changes) and
  Select all / Select none per group. A module's details open inline under
  its group; a line under the list says what the last click changed.
- **Summary** (`SummaryPage.qml`): an editable receipt: every selection by
  category, each removable in place, with Change to go back to its category.
- **Review** (`ReviewPage.qml`): what the run costs (password, restart, undo),
  then Will install, Will change and Left alone; every step, with its files
  and commands, folds under "Technical details".
- **Apply** (`ApplyPage.qml`): the running step large, the steps and log
  below. When done: a result banner, then Restart now, Undo these changes,
  Open the log and a few things to try first; the steps and log fold away.

After a Bindle upgrade with something new, the window opens in What's new
mode instead: What's new → Review → Apply, over its picks
([UPGRADE-PATH.md](UPGRADE-PATH.md)). Each screen reads the same data the
terminal wizard does (`bindle_run::present`, `plan::resolve`). On uQ the
hand-off to uQ's Welcome is automatic when the first-login wizard ends (see
below); there is no button for it. The update window (`bindle-gui
--upgrade`) is a second set of screens in the same binary, below.

## The update window

`bindle-gui --upgrade` (opened by `bindleupgrade --gui`, which passes
`--dry-run` / `--sandbox` on) is bindleupgrade with the wizard's look: the
same QML module, theme, `Glyph`, `Panel`, `Pill`, `Notice`, `ReviewList`,
`StepBar` and `Disclosure`, from a second entry point (`UpgradeMain.qml`)
in the same binary rather than a separate one, so it installs, themes and
screenshots like the wizard. Steps: Check → Update → Done.

- **Check** (`UpgradeCheckPage.qml`): the checks as they happen, then what
  the update costs (password, restart, snapshot), the Arch news since the last
  upgrade (a post asking for a manual step first, with Read), and every stage
  with work, its details and a switch: leave the stage out, or answer its
  question (upgrade the packages, remove orphans, "I've read it" for the
  news), or install firmware. Stages with nothing to do share one quiet list.
- Stages a switched-off question holds (the news unread, updating on
  battery, the system upgrade off) show as held, with why, and don't count
  as work; so do the preflight holds (space, uQ's pin).
- **Update** (`UpgradeRunPage.qml`): each stage's status beside the output,
  then the result table, Restart now, "Update the AUR" and "Finish in a
  terminal" (both `bindle in-terminal upgrade`), and Open the log (every line,
  in `~/.cache/bindle/upgrade-*.log`; the helper's own log is root's). Also
  "Don't restart yet" (a DKMS module or boot image missing; no Restart
  button then), Open Bindle (new features), and Go on after Bindle updated
  itself (the new window does the rest).

`bindle-upgrade` stays free of Qt. `src/upgrade.rs` (the Check model: the
probe, the switches, `Upgrade::check`) and `src/upgrade/run.rs` (the run's
state and the window's `Ui`, which answers each stage's question from its
switch and anything else no) are plain Rust with unit tests;
`src/upgrade_backend.rs` is the cxx-qt bridge, as `backend.rs` is for the
wizard. The plan is `bindle_upgrade::plan` with `windowed: true`: the AUR,
whose helper reviews and asks for a password on a terminal, is reported and
handed to one. Screenshots use built-in sample systems
(`src/upgrade/sample.rs`: one with something in every stage, one held back
by /boot space, uQ's pin and battery, with a restart that isn't safe) and a
dry run (`u1`…`u5` per theme).

## uQ integration

1. **Catalog**: a `uq` desktop in `catalog/desktops.toml` (recommended, so
   pre-selected; based on Hyprland, with its own essentials and apps) and a
   `uq` module in `catalog/arch/desktop.toml`, which installs `uq`,
   `uq-hyprland-pin` and `bibata-cursor-theme-bin` from the AUR, runs
   `uqctl setup --apps` as the user and switches to uQ's login screen
   (greetd). uQ's own apps (scdr, crumbs, noti-fi and others) are their own
   modules.
2. **Hand-off** (`crates/bindle-cli/src/uq_handoff.rs`): while the
   first-login wizard is open in a uQ session, Bindle holds
   `$XDG_RUNTIME_DIR/uq/welcome.defer` so uQ's Welcome waits; when the wizard
   ends it removes the file and runs `uqctl welcome` (uQ Phase 5).
3. **Settings page**: `bindle first-login install` writes
   `/usr/share/uq/settings/pages/bindle/` (`page.toml`, `Page.qml`, from
   `uq/pages/bindle/`) and links `/usr/lib/bindle/bindle` to Bindle, giving uQ
   Settings a "Re-run setup" page: the profile and when it last ran, "Pick
   apps again" (`bindle open --detach`) and "Check for missing pieces".

## Architecture

- **`crates/bindle-run`** (new library): the apply orchestrator, helper client,
  protocol, release downloads, AUR builds, snapshots and selection files, moved
  out of `bindle-cli`. Apply reports through a `Progress` trait (snapshot saved,
  step started, output line, step failed, finished) instead of `println!`. The
  CLI's implementation prints exactly what it prints today.
- **`crates/bindle-cli`**: CLI, terminal wizard and the root `helper`; depends
  on `bindle-run`.
- **`crates/bindle-gui`** (new binary): cxx-qt backend QObject (facts, catalog,
  selection, plan and progress as properties and signals) plus the QML module.
  Apply runs on a worker thread; progress reaches QML as signals. Laid out like
  `hauler/crates/hauler-desktop` (`build.rs` with `CxxQtBuilder`, `qml/`,
  `src/`).

## Launching

- `bindle first-login run` starts `bindle-gui` when there is a graphical
  session (`WAYLAND_DISPLAY` or `DISPLAY`), `bindle-gui` is installed next to
  `bindle`, and Kirigami is installed; otherwise it opens the terminal wizard as
  now. uQ's Hyprland config can launch it the same way.
- `bindle iso` adds `kirigami`, `qt6-declarative` and `breeze-icons` to the install so the
  window works at first login on any desktop, not only Qt ones.
- If pkexec has no polkit agent to ask for the password (some window managers
  before Bindle installs one), the window offers to continue in a terminal with
  `bindle apply --from <selection>`.
- `first-login install`, `install.sh`, the archinstall plugin and the release
  workflow ship `bindle-gui` and the polkit policy alongside `bindle`. The
  AppImage keeps opening the terminal wizard for now (it is about 2.4 MB;
  bundling Qt and Kirigami would add tens of megabytes).

## Testing

- Rust unit tests for the backend's models and the token-to-theme mapping.
- Offscreen screenshots (`QT_QPA_PLATFORM=offscreen`) of every screen in every
  uQ theme variant, mirroring uQ's per-theme screenshot tests.
- Icons are Breeze names drawn by `Glyph.qml` in one theme colour: the backend
  rewrites the SVG's colours (`src/glyph.rs`), which works without a GPU and
  on any background. Use outline-style names; a full-colour icon becomes a
  solid square. Qt logs QML errors to the journal: run with
  `QT_FORCE_STDERR_LOGGING=1` to see them.
- Apply end to end through `--sandbox` (the helper's `--fake-commands`).
- The VM test screenshots the window at first login, including on uQ.

## Phases (one commit each)

Done: 1 (engine), 2 (Welcome to Review), 3 (Apply and Done, pkexec with a
polkit action, terminal fallback when no polkit agent runs), 4 (first login
and the app-menu entry open the window; releases, `install.sh`, the ISO
plugin and `first-login install` ship it; `bindle iso` adds the Qt runtime),
5 (uQ catalog) and 6 (the uQ hand-off and the uQ Settings page; 0.1.8). The window applies
unattended: the Review screen is the confirmation, so AUR helpers skip their
own prompts, and output from AUR builds goes to the window's stdout rather
than its log (the helper's output, pacman's included, is shown).


1. Extract `bindle-run` and the `Progress` trait; CLI output unchanged.
2. `bindle-gui`: Welcome, Profile, Choose, Review; uQ token theming;
   offscreen screenshot tests.
3. Apply and Done screens; pkexec, polkit policy, terminal fallback.
4. Launch from first login; `bindle iso` adds the Qt runtime; ship
   `bindle-gui` in releases, `install.sh` and the ISO plugin.
5. uQ catalog: `uq` desktop and module, uQ's apps as modules.
6. uQ Settings page and Welcome hand-off.
