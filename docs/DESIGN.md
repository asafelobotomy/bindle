# Bindle design

> A bindle is a small bundle of possessions tied to a stick and carried by
> someone on the move. Bindle is what you carry from one fresh install to the next.

## Goal

One file you download onto a freshly installed distro, run, and walk through a
wizard that installs packages, drivers and tools, applies tuned configuration
(zram, I/O schedulers, pacman, mirrors…) and leaves the system ready to use.
Arch Linux first; other families later.

## Constraints of a *fresh* install

These shaped every decision below:

| Reality on day 0 | Consequence |
|---|---|
| Often **TTY only** (archinstall minimal profile, no DE yet) | Primary UI is a **terminal wizard**; a GUI is an optional second frontend. |
| No polkit agent without a desktop session | Privilege escalation must work via `sudo` as well as `pkexec`. |
| `fuse3` (`fusermount3`) not in a pacstrap base | AppImage may not mount: document `--appimage-extract-and-run`, also publish the bare static binary. |
| No AUR helper; `makepkg` refuses to run as root | AUR work runs **as the user**; only `pacman`/file writes run as root. |
| User may re-run after a failure | Every step must be **idempotent**; planning skips work that's already done. |

## Prior art

- **archinstall** profiles: proves the "pick a profile, tweak, apply" flow; install-time only.
- **CachyOS Hello / EndeavourOS Welcome / Garuda Assistant**: post-install helpers, each tied to its own distro and repos.
- **XZram** (ours): CLI-first Rust workspace with **staged apply**, a polkit **helper**,
  **snapshots before apply**, read-only `doctor`, and a hardware-aware recommendation matrix.
  Bindle reuses those patterns and ships a simplified zram module.

## Architecture

```
┌──────────────── bindle (single static binary, packaged as AppImage) ───────────────┐
│  frontends:  TUI wizard (ratatui) │ CLI (detect/list/plan/apply) │ GUI (later)       │
│                         └────────────── all produce a Selection ──────────────┘       │
│  bindle-core:  detect ─► Facts ─┐                                                  │
│                catalog (TOML) ──┼─► plan::resolve ─► Plan { steps[] }  (pure, JSON) │
│                builtins ────────┘                                                  │
│  executor (Phase 1):  user-side runner  ◄──►  root helper session (sudo / pkexec)   │
└────────────────────────────────────────────────────────────────────────────────────┘
```

### Catalog: data, not code

Modules live in `catalog/<distro>/<category>.toml` and are compiled into the
binary (`include_str!`), so the AppImage needs no data files. A module declares
packages, AUR packages, units to enable, groups, drop-in files, pacman.conf
options, repos, commands, `requires`/`conflicts`, `when` conditions, `profiles`
and a `risk` level. See [CATALOG.md](CATALOG.md).

Where the right answer depends on hardware, a module names a **builtin**
(`gpu`, `microcode`, `zram`, `aur-helper`) that expands from detected facts.

### Plan / apply split

`plan::resolve` is pure: selection + facts + current file contents → ordered
steps, with no side effects. That gives:

- a full **dry-run review** before anything runs (wizard Review screen, `bindle plan`);
- `--json` output any frontend (Tauri GUI, scripts) can consume;
- **idempotence**: installed packages and byte-identical files are dropped from the plan;
- fixed phase order: pacman.conf → one `pacman -Syu --needed` transaction → files →
  units → groups → root commands → initramfs → AUR (as user) → user commands.

### Privilege model

The orchestrator runs as the user. Root steps go to **one helper session**
(the same binary in a hidden `helper` mode), started once through `sudo` (TTY) or
`pkexec` (desktop session); it reads steps as JSON lines and streams output back.
A FUSE mount is private to the user who mounted it, so from an AppImage the
binary is first copied to a private directory root can read. A version
handshake makes sure both sides speak the same protocol. The helper is as
powerful as sudo; see [APPLY.md](APPLY.md#trust-model).

### Safety

- Prefer **drop-in files** (`sysctl.d`, `makepkg.conf.d`, `udev/rules.d`,
  `tmpfiles.d`) with a `# Managed by Bindle` header over editing distro files.
- `pacman.conf` is the one in-place edit: line-level, comment-preserving, unit-tested.
- Before applying, snapshot every file that will be touched (XZram pattern) under
  `/var/lib/bindle/snapshots/<timestamp>/` with a manifest, so `bindle undo` can restore it. Details: [APPLY.md](APPLY.md).
- Respect existing setups: the zram builtin leaves an existing
  `zram-generator.conf` alone, defers to another `sysctl.d` file that already
  sets `vm.swappiness` (e.g. XZram's), and skips the zswap tmpfiles rule when
  `zswap.enabled=0` is already on the kernel command line.
- Risk levels (`safe` / `moderate` / `advanced`) are shown in the wizard; advanced modules are never in a profile.

### Relationship with XZram

Bindle's `zram` module covers the common case: zram-generator sized by RAM
tier (XZram's matrix), zstd, swap-tuned sysctls, zswap off. XZram stays the
tool for ongoing swap management (overflow swapfiles, priorities, snapshots,
doctor). A later module can offer "install XZram" for users who want it.

## Starting point: an archinstall desktop install

Bindle targets the system archinstall leaves behind. It recovers archinstall's
choices (logged config, or rebuilt from pacman.log), shows them first, treats
them as done, and adds per-desktop essentials for the 21 desktops archinstall
offers. Details: [ARCHINSTALL.md](ARCHINSTALL.md).

## Detection (no root)

`/etc/os-release`, `/proc/{cpuinfo,meminfo,mounts,swaps,cmdline}`,
`/sys/class/drm/*/device/vendor` (GPU PCI vendor), `/sys/block/*/queue/rotational`,
`/sys/class/power_supply`, `/sys/class/bluetooth`, `systemd-detect-virt`,
the `LoaderInfo` EFI variable (bootloader; `/boot` is often root-only),
`/etc/pacman.conf` and `pacman -Qq`.

## Frontends

1. **TUI (now)**: works in a bare TTY, over SSH, and in any terminal; `AppRun`
   opens a terminal emulator when the AppImage is double-clicked.
2. **GUI (Phase 3)**: Tauri 2 + React, matching emobie/SonicSound, driving
   `bindle-core` directly. The webkit2gtk runtime makes it heavy and it needs a
   desktop session, so it complements the TUI and doesn't replace it.

## Growing past Arch

`Facts::is_arch_family` already accepts CachyOS, EndeavourOS and other
`ID_LIKE=arch` systems. Other families get their own `catalog/<family>/` plus a
package-manager backend in the executor; the planner and frontends are distro-agnostic.
