# Agent notes for Bindle

Fresh-install setup wizard for Linux (Arch first). Rust workspace; TUI-first; AppImage.

## Layout

| Path | What |
|---|---|
| `crates/bindle-core` | `detect` (facts, no root), `install` (archinstall log / pacman.log reconstruction), `desktops`, `catalog` (TOML, validated), `builtins` (hardware-computed modules; `drives` generates one module per mountable drive), `plan` (pure resolver; `plan/content.rs` per-module content), `whatsnew` (pure "What's new" diff + `catalog/changes.toml`), `pacman_conf`, `fstab` |
| `crates/bindle-run` | apply engine shared by frontends: `apply` (orchestrator, `Progress` trait), root `helper`, JSON `protocol`, `client`, `snapshot` (also the per-module record), `record` (the ledger), `whatsnew` (record, picks), `whatsnew_state` (skip, hide), `sys`, `selection`, `present` (text every frontend shows) |
| `crates/bindle-gui` | `bindle-gui`: desktop wizard, cxx-qt + Kirigami (`qml/`, `src/model.rs` holds the wizard state without Qt, `src/theme.rs` reads uQ's tokens; `--upgrade`: the bindleupgrade window, `UpgradeMain.qml` over `src/upgrade.rs` + `upgrade_backend.rs`). Not a default workspace member: needs Qt 6 and Kirigami |
| `crates/bindle-cli` | `bindle` binary: clap CLI, ratatui wizard (`src/tui`), `apply.rs` (terminal frontend over `bindle-run`), `whatsnew_cmd.rs` + `tui/whatsnew.rs`, `firstlogin.rs`, `terminal.rs` |
| `crates/bindo` | `bindo` recipe runner library: TOML `recipe` format, `book` (shipped `recipes/*.toml`, installed modules' `[[module.recipe]]`, then `/usr/share`, `/etc`, `~/.config` overrides), `args`, `show` (listing, help, completions), `exec` (pkexec/run0, never sudo). CLI in `bindle-cli/src/bindo_cmd.rs` |
| `crates/bindle-upgrade` | `bindleupgrade`: `probe` (looks, no root; `probe_system.rs`), `system` (pure parsers: df, dkms, power, pin, vercmp), pure `plan` (`plan_pre`, `plan_pkgs`, `plan_after`, `plan_close`), `prefs` (`upgrade.toml`), `releases` (pins by install source), `selfupdate` (signed release), `setup` (new features, drift), `run` (+`run_actions`; root helper + user commands, keeps going), `report`. CLI in `bindle-cli/src/upgrade_cmd.rs` |
| `catalog/` | Module data + `desktops.toml`, embedded at compile time; add files to `ARCH_FILES`; `make check-packages` |
| `packaging/appimage` | `AppRun`, desktop file, `build-appimage.sh` |
| `packaging/archinstall` | `bindle_plugin.py` (embedded into `bindle iso`) |
| `install.sh` | curl bootstrap for the live ISO → `bindle iso` |
| `crates/bindle-cli/src/iso.rs`, `tui/iso.rs` | live-ISO installer flow; preset in `bindle-core/src/install/preset.rs` |
| `tests/vm/` | QEMU end-to-end install test (`run.sh`, guest scripts); artifacts in git-ignored `.work/` |
| `branding/` | `bindle.svg` (the icon) and `png/` sizes from `scripts/icons.sh`; rerun it after editing the SVG (a unit test checks) |
| `docs/` | DESIGN (why), CATALOG (schema), ROADMAP, ISO, ARCHINSTALL (detection, first login, welcome apps), APPLY, DRIVES (fstab, mounting other drives), BINDO (recipes), UPGRADE (bindleupgrade), UPGRADE-PATH (What's new after upgrading Bindle), VM-TEST |

## Verify

```bash
make test          # unit tests (catalog validation runs here)
make lint          # fmt --check + clippy -D warnings
make gui-test      # bindle-gui clippy + tests (needs qt6-declarative, kirigami)
make gui-shots     # every screen offscreen, per uQ theme fixture, into dist/shots/
cargo run -p bindle-cli -- detect
cargo run -p bindle-cli -- plan --profile gaming
cargo run -p bindle-cli -- bindo                # list recipes; `bindo -n <recipe>` prints, runs nothing
cargo run -p bindle-cli -- upgrade --check      # what bindleupgrade would do; `-n` prints every command
cargo run -p bindle-cli -- upgrade --gui -n     # the update window, dry run (needs bindle-gui built)
cargo run -p bindle-cli -- whats-new --list      # what this Bindle adds or changes here; `--sandbox <dir> --yes` rehearses applying it
```

`apply`, `undo`, `first-login install`, `upgrade` and admin `bindo` recipes change the system. While developing,
use the hidden `--sandbox <dir>` (apply/undo/wizard) or `--root <dir>`
(first-login), `upgrade --dry-run`/`--sandbox <dir>` and `bindo --dry-run`: they run unprivileged and only record commands. Never test apply
on the host with sudo; use an Arch VM. `bindle iso` refuses to run outside the
live ISO (archinstall wipes disks); use `bindle iso --dry-run`, or the real thing in a VM with `tests/vm/run.sh` (docs/VM-TEST.md). Bindle itself refuses to run apply as root.

## Conventions
- Keep `plan::resolve` pure; new behaviour that touches the system belongs in the executor.
- Prefer drop-in files with `# Managed by Bindle` over editing distro files.
- New hardware-dependent logic is a builtin with unit tests driven by `Facts`.
- Keep files at or under ~400 lines (XZram convention).
