# Bindle audit — 2026-11-08

## Status (updated after remediation)

| Finding | Status | Commit |
|---|---|---|
| H1 non-UTF-8 output kills pacman | Fixed | 9b04ffb |
| H2 GPU ignores installed NVIDIA driver | Fixed (incl. derivative kernel packages) | 9b04ffb, e0cc44a |
| H3 conflicts abort the transaction | Fixed: skip, or replace after confirmation | 9b04ffb, 2111ff1 |
| M1 release authenticity | Fixed: GPG-signed SHA256SUMS, verified by install.sh (key 75C4…69FC) | see git log |
| M2 unpinned checkout | Fixed | e0cc44a |
| M3 weak live-ISO guard | Fixed | fad4dae |
| M4 silent mode disk checks | Fixed | fad4dae |
| M5 undo after resumed apply | Fixed (session undo) | fcacd2f |
| M6 SUDO_USER target user | Fixed | fcacd2f |
| M7 restore stops at first error | Fixed | fcacd2f |
| M8 AUR bootstrap trust | Fixed (pinned commit + checksum, staging, review) | 42a4bf6 |
| M9 derivatives | Fixed (distro:arch gating, driver tool note) | 2111ff1 |
| M10 Secure Boot vs DKMS | Not applicable: stock Arch doesn't enforce module signatures or lockdown | — |
| M11 snapper chain not resumable | Fixed | a70520e |
| M12 preset merge drops multilib | Fixed | fad4dae |
| M13 supply-chain gate | Fixed; zbus 6 move still open (advisories ignored in deny.toml) | e0cc44a |
| M14 test gaps | Fixed: CLI + helper protocol tests, per-phase unit tests | 4ed3cf6 and others |
| M15 release toolchain / MSRV | Fixed | e0cc44a |
| Low: pacman.conf heuristics | Fixed; repos enabled via `Include`d files still not detected | a70520e |
| Low: concurrency, signals, dedup, helper input, log | Fixed | fcacd2f |
| Low: first-login lock, cleanup, log parsing | Fixed | 7db41fc |
| Low: catalog validation | Fixed | a70520e |
| Low: detection | Fixed except Haswell-and-older Intel VA-API (needs PCI generation detection) | a70520e |
| Low: TUI | Fixed | 4ed3cf6 |
| Low: project files, VM harness | Fixed | 4ed3cf6 |
| Low: Dependabot runs cancelled | Open: jobs never get a runner (0 steps); a repository/account Actions setting, not code | — |

Not yet verified end to end: a real apply on Arch, and `tests/vm/run.sh`.

---

Scope: whole repo at `f861d48` (v0.1.0). Baseline: `cargo build --locked` ok, 72 unit tests pass, `make lint` clean.
Method: four read-only passes (root apply path; ISO + first login; detection + catalog + TUI; CI/deps/packaging/docs), then spot-verification of the top findings against source. `apply`/`undo` were exercised only with `--sandbox` (confirmed it touched nothing outside the sandbox). Catalog package checks ran on a Manjaro host, so repo-name misses for `reflector`/`linux-zen*` are host artefacts.

No unprivileged→root escalation was found beyond the documented "helper = sudo" model: selection files can only name validated catalog IDs, the helper re-checks paths/names, `write_atomic` is exclusive and refuses symlinks.

## High

| # | Finding | Where | Failure |
|---|---|---|---|
| H1 | Non-UTF-8 output stops the reader; child dies of SIGPIPE mid-transaction | `crates/bindle-cli/src/apply/sys.rs:159` (`lines().map_while(Result::ok)`) | One latin-1 byte from pacman/makepkg → pipe closed → pacman killed mid-upgrade, stale db lock, half-upgraded system. Reproduced with a scratch program. Fix: `read_until(b'\n')` + `from_utf8_lossy`, never stop draining. |
| H2 | GPU builtin ignores installed NVIDIA drivers | `crates/bindle-core/src/builtins/gpu.rs:53` | User already on `nvidia`/`nvidia-open`/`nvidia-dkms` gets `nvidia-open-dkms` (conflicts) → `pacman --noconfirm` answers "remove?" with N → **whole** `-Syu` transaction aborts, every module's packages lost. Pre-Turing branch (`:58`) likewise adds nouveau Vulkan alongside an installed `nvidia-580xx`. Fix: skip when any `NVIDIA-MODULE` provider is installed. |
| H3 | Package conflicts with installed packages abort the transaction | `catalog/arch/development.toml:14` (rustup note is wrong), nodejs, `hardware.toml` pipewire/tlp | `rustup` vs `rust`, `nodejs-lts-*` vs `nodejs`, `pipewire-pulse` vs `pulseaudio`, `pipewire-jack` vs `jack2`, `tlp-pd` vs `power-profiles-daemon` (installed outside archinstall) → same all-or-nothing failure. Fix: planner pre-flight against `facts.installed` (skip module with reason), or batch per module. |

## Medium

| # | Finding | Where |
|---|---|---|
| M1 | `install.sh` verifies a checksum fetched from the same release, so it detects corruption but doesn't prove who published the binary; it runs as root on a disk-wiping path. No `--proto '=https'`; `http://` base URL accepted; `BINDLE_VERSION` unvalidated. AppImages ship with no checksum. | `install.sh:49-52`, `.github/workflows/release.yml` |
| M2 | `actions/checkout@v5` not SHA-pinned (others are); release job has `contents: write`. | `ci.yml:22`, `release.yml:19` |
| M3 | Live-ISO guard is just `/run/archiso` exists; `BINDLE_FORCE=1` in install.sh isn't restricted to `--dry-run` as its comment claims. | `crates/bindle-cli/src/iso.rs:62`, `install.sh:32` |
| M4 | Silent ISO mode checks `--config` is present, not that it has a non-empty `disk_config`; target devices aren't echoed/validated before wipe. | `iso.rs` (~116) |
| M5 | `undo` (latest) after a resumed apply only reverts the second run; first run's pacman.conf edits stay. Reproduced in sandbox. | `apply/mod.rs` `snapshot_plan`/`undo` |
| M6 | Target user taken from `SUDO_USER` before uid → after `sudo -u dev -i`, groups (docker/wheel/libvirt) go to the wrong user and the plan is computed for them. | `crates/bindle-core/src/detect/mod.rs:129` |
| M7 | Restore aborts on first per-file error; later files not restored, units not disabled, no summary. Created dirs never removed. | `apply/snapshot.rs` `restore_files`, `helper.rs` Restore |
| M8 | AUR bootstrap builds unpinned default branch, then `pacman -U` as root (from a user-writable cache); paru runs `--skipreview`; `-debug` packages installed too. | `apply/user.rs:46,71`, `helper.rs:152` |
| M9 | Derivatives (Manjaro/CachyOS/Endeavour) only get a warning; `reflector`/`kernel-zen`/nvidia packages then fail the transaction. | `detect/mod.rs:193`, `main.rs:337` |
| M10 | No Secure Boot detection; unsigned `nvidia-open-dkms` won't load while nouveau is blacklisted → no driver. (plausible) | `builtins/gpu.rs` |
| M11 | Snapper command chain shares one `unless` that becomes true after step 3; a later failure leaves `/.snapshots` unmounted and re-runs skip everything. `btrfs subvolume delete` isn't undoable. | `catalog/arch/security.toml` snapper |
| M12 | Preset merge replaces arrays: a user `optional_repositories` drops gaming's `multilib`, yet the summary still shows it. | `crates/bindle-core/src/install/preset.rs` `merge` |
| M13 | Supply-chain gate missing: no `deny.toml`/`cargo deny` in CI. Today: `async-executor 1.14.0` yanked; zbus 5's `zvariant`/`zbus_names`/`zvariant_derive` flagged unmaintained (merged into zbus 6). | `Cargo.lock`, `ci.yml` |
| M14 | Test gaps: no tests in `apply/helper.rs`, `protocol`, `client.rs`, `user.rs`, `iso.rs`, `main.rs`; no CLI integration tests; VM test not in CI. | `crates/bindle-cli` |
| M15 | Release toolchain unpinned (`pacman -Syu` in rolling container); `workflow_dispatch` skips the tag check (publish step is guarded). MSRV never tested. | `release.yml` |

## Low

- **pacman.conf heuristics** (`bindle-core/src/pacman_conf.rs:47`): `option_key` matches prose comments (`# Color output is nice` gets replaced); `enable_repo` uncomments any `# Foo = x` after `#[multilib]`; `Include`d files ignored (duplicate `[multilib]` possible); CRLF → LF. Validate with `pacman-conf` before rename.
- **Concurrency/signals**: no lock in the helper; snapshot dir `exists()`+`create_dir_all` race; ids sort `-10` < `-2`; Ctrl-C reaches pacman with no cleanup.
- **Snapshot dedup** (`apply/mod.rs:76`): `dedup()` is adjacent-only → a package-shipped file can be deleted on restore.
- **Helper hardening**: `EditPacmanConf` keys/values not validated (newline injection from trusted callers only); Restore doesn't `check_system_path`; `--log` follows symlinks and open failure is silent; describe says `-Syu --needed` without saying it's a full upgrade.
- **First login**: launch lock goes stale after 600 s (`session.rs:18`) while the wizard can run 30+ min → a second wizard possible. System-wide autostart/service/profile.d files stay until `first-login remove`.
- **Log parsing**: `install/mod.rs:76` `read_to_string().unwrap_or_default()` — one non-UTF-8 byte silently drops detection; `pacman_log` misses `--root=` and value-taking flags like `--ignore`.
- **ISO**: workdir config (may hold disk layout / user secrets, 0600 on tmpfs) never deleted; archinstall version check only warns when newer; preset uses archinstall display strings.
- **Detection**: SMBIOS chassis 30 (tablet) missing; USB stick flips `ssd`; Haswell-and-older Intel need `libva-intel-driver`; Hyprland gets two polkit agents; `nodejs-lts-krypton` will go stale.
- **Catalog validation** (`catalog.rs`): accepts self-conflicts, duplicate ids, empty ids, `-`-prefixed names, `..` paths, setuid modes, duplicate file paths across modules, conflicting defaults, `aur` without `requires aur-helper`. `desktops::all().unwrap_or_default()` swallows errors. `check-packages.py` skips AUR and unit names.
- **TUI**: detail pane is 5 lines at 80×24, not scrollable — notes/"not applicable" reasons hidden; module conflicts only surface on Review.
- **Project files**: no SECURITY.md, CHANGELOG, CONTRIBUTING; no enforced 400-line check (`tui/mod.rs` at 385); Dependabot runs cancelled.
- **VM harness**: stale servers on 8765/8766 reused silently; NOPASSWD sudoers written before `@includedir` check.

## Verified fine

Sandbox isolation; helper handshake, `valid_name` + `--`; snapshot-before-change (fail-closed); exclusive temp + rename with mode set first; root refusal; no input-reachable panics; ISO workdir 0700/0600; first-login guards (`ConditionUser=!@system`, UID_MIN, SSH skip); `welcome.rs` doesn't follow symlinks; `install.sh` is truncation-safe with `set -euo pipefail`; AppImage tools pinned + sha256-verified; workflow permissions least-privilege, no untrusted `${{ }}` in `run:`; all enabled unit names exist; Turing cutoff and hybrid detection correct; docs match `--help`.

## Suggested order

1. H1 (one-function fix, prevents bricked upgrades).
2. H2 + H3 via one planner pre-flight against installed packages / conflicts.
3. M1–M4: sign releases, pin checkout, harden the ISO guard and silent-mode disk check.
4. M5–M7: undo/restore robustness; M6 uid-based user.
5. CI: `cargo deny`, `cargo update -p async-executor`, CLI + helper protocol tests.
