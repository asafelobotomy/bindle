# GUI

A desktop window for the wizard, replacing the terminal at first login on
graphical sessions. The terminal wizard stays for TTYs, SSH and as a fallback.
Supersedes the Tauri plan in ROADMAP Phase 3.

uQ (`asafelobotomy/uq`) is Bindle's main desktop. Its design spec makes Bindle
uQ's first-run installer: Bindle installs the `uq` profile, hands off to the
Welcome page in `uq-settings`, and adds a "re-run setup" page there. The GUI
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

Welcome (install summary, already set up, suggestions) → Profile → Choose
(categories, modules, details, search) → Review (counts, replacements, skips,
steps) → Apply (progress, per-step status, log) → Done (reboot/log-out note,
undo hint, and on uQ "Open uQ Welcome"). Each screen reads the same data the
terminal wizard does: `print::install_lines`, `print::module_details`,
`plan::resolve`.

## uQ integration

1. **Catalog**: a `uq` desktop in `desktops.toml` (Hyprland plus the `uq`
   package; its essentials skip waybar, hyprpaper and the other pieces uq-shell
   replaces), a `uq` profile, and a "uQ apps" category grouping the integrated
   apps (noti-fi, scale_tail, XZram, scdr, appOFholding, crumBS, hauler,
   emobie, oysterAV, romforge). The profile installs `uq-hyprland-pin` and
   `uq` from pinned GitHub release packages and runs `uqctl setup` as the user.
   **Blocked on uQ publishing release packages**; apps that already ship
   releases can be added first.
2. **Hand-off**: after a `uq` profile apply, Done offers to open uq-settings on
   its Welcome page (uQ Phase 5).
3. **Settings page**: the wizard's QML lives in a module (`org.bindle.wizard`)
   that `bindle-gui` loads and that is also installed as
   `/usr/share/uq/settings/pages/bindle/` with a `page.toml`, giving
   uq-settings a "Re-run setup" page (uQ Phase 3 extension points).

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
- `bindle iso` adds `kirigami` and `qt6-declarative` to the install so the
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
- Apply end to end through `--sandbox` (the helper's `--fake-commands`).
- The VM test screenshots the window at first login, including on uQ.

## Phases (one commit each)

Done: 1 (engine), 2 (screens up to Review; the Apply button says applying
from the window comes next).


1. Extract `bindle-run` and the `Progress` trait; CLI output unchanged.
2. `bindle-gui`: Welcome, Profile, Choose, Review; uQ token theming;
   offscreen screenshot tests.
3. Apply and Done screens; pkexec, polkit policy, terminal fallback.
4. Launch from first login; `bindle iso` adds the Qt runtime; ship
   `bindle-gui` in releases, `install.sh` and the ISO plugin.
5. uQ catalog: `uq` desktop, "uQ apps" category (apps with releases now), `uq`
   profile once uQ publishes packages.
6. uq-settings page and Welcome hand-off, once uQ Phases 3 and 5 land.
