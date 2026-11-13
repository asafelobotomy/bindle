# Agent notes for Bindle

Fresh-install setup wizard for Linux (Arch first). Rust workspace; TUI-first; AppImage.

## Layout

| Path | What |
|---|---|
| `crates/bindle-core` | `detect` (facts, no root), `install` (archinstall log / pacman.log reconstruction), `desktops`, `catalog` (TOML, validated), `builtins` (hardware-computed modules; `drives` generates one module per mountable drive), `plan` (pure resolver), `pacman_conf`, `fstab` |
| `crates/bindle-run` | apply engine shared by frontends: `apply` (orchestrator, `Progress` trait), root `helper`, JSON `protocol`, `client`, `snapshot`, `sys`, `selection`, `present` (text every frontend shows) |
| `crates/bindle-gui` | `bindle-gui`: desktop wizard, cxx-qt + Kirigami (`qml/`, `src/model.rs` holds the wizard state without Qt, `src/theme.rs` reads uQ's tokens). Not a default workspace member: needs Qt 6 and Kirigami |
| `crates/bindle-cli` | `bindle` binary: clap CLI, ratatui wizard (`src/tui`), `apply.rs` (terminal frontend over `bindle-run`), `firstlogin.rs`, `terminal.rs` |
| `catalog/` | Module data + `desktops.toml`, embedded at compile time; add files to `ARCH_FILES`; `make check-packages` |
| `packaging/appimage` | `AppRun`, desktop file, `build-appimage.sh` |
| `packaging/archinstall` | `bindle_plugin.py` (embedded into `bindle iso`) |
| `install.sh` | curl bootstrap for the live ISO → `bindle iso` |
| `crates/bindle-cli/src/iso.rs`, `tui/iso.rs` | live-ISO installer flow; preset in `bindle-core/src/install/preset.rs` |
| `tests/vm/` | QEMU end-to-end install test (`run.sh`, guest scripts); artifacts in git-ignored `.work/` |
| `branding/` | `bindle.svg` (the icon) and `png/` sizes from `scripts/icons.sh`; rerun it after editing the SVG (a unit test checks) |
| `docs/` | DESIGN (why), CATALOG (schema), ROADMAP, ISO, ARCHINSTALL (detection, first login, welcome apps), APPLY, DRIVES (fstab, mounting other drives), VM-TEST |

## Verify

```bash
make test          # unit tests (catalog validation runs here)
make lint          # fmt --check + clippy -D warnings
make gui-test      # bindle-gui clippy + tests (needs qt6-declarative, kirigami)
make gui-shots     # every screen offscreen, per uQ theme fixture, into dist/shots/
cargo run -p bindle-cli -- detect
cargo run -p bindle-cli -- plan --profile gaming
```

`apply`, `undo` and `first-login install` change the system. While developing,
use the hidden `--sandbox <dir>` (apply/undo/wizard) or `--root <dir>`
(first-login): they run unprivileged and only record commands. Never test apply
on the host with sudo; use an Arch VM. `bindle iso` refuses to run outside the
live ISO (archinstall wipes disks); use `bindle iso --dry-run`, or the real thing in a VM with `tests/vm/run.sh` (docs/VM-TEST.md). Bindle itself refuses to run apply as root.

## Conventions
- Keep `plan::resolve` pure; new behaviour that touches the system belongs in the executor.
- Prefer drop-in files with `# Managed by Bindle` over editing distro files.
- New hardware-dependent logic is a builtin with unit tests driven by `Facts`.
- Keep files at or under ~400 lines (XZram convention).
