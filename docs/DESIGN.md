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
| No polkit agent without a desktop session | Every approval goes through `pkexec`; on a console it prompts as text. Only the unattended ISO install (arch-chroot, no polkit) uses its passwordless sudo grant. |
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
│  frontends:  TUI wizard (ratatui) │ CLI (detect/list/plan/apply) │ bindle-gui*      │
│                         └────────────── all produce a Selection ──────────────┘       │
│  bindle-core:  detect ─► Facts ─┐                                                  │
│                catalog (TOML) ──┼─► plan::resolve ─► Plan { steps[] }  (pure, JSON) │
│                builtins ────────┘                                                  │
│  bindle-run:   apply (Progress) ─► user-side steps ◄──► root helper (pkexec)       │
└────────────────────────────────────────────────────────────────────────────────────┘
```

\* `bindle-gui` (Kirigami) is a separate binary linked against the system's Qt 6,
installed next to `bindle`; it is not in the AppImage.

### Catalog: data, not code

Modules live in `catalog/<distro>/<category>.toml` and are compiled into the
binary (`include_str!`), so the AppImage needs no data files. A module declares
packages, AUR packages, units to enable, groups, drop-in files, pacman.conf
options, repos, commands, `requires`/`conflicts`, `when` conditions, `profiles`
and a `risk` level. See [CATALOG.md](CATALOG.md).

Where the right answer depends on hardware, a module names a **builtin**
(`gpu`, `nvidia-legacy`, `microcode`, `zram`, `aur-helper`, `desktop`,
`everyday`, `spellcheck`, and `mount:<uuid>` for each drive module) that
expands from detected facts.

### Plan / apply split

`plan::resolve` is pure: selection + facts + current file contents → ordered
steps, with no side effects. That gives:

- a full **dry-run review** before anything runs (wizard Review screen, `bindle plan`);
- `--json` output any frontend (the GUI, scripts) can consume;
- **idempotence**: installed packages, enabled units, groups the user is already
  in, byte-identical files and commands whose `unless` file checks already hold
  are dropped from the plan, so re-applying the same selection plans nothing;
- fixed phase order: pacman.conf → one `pacman -Syu --needed` transaction → files →
  fstab → units → groups → root commands → initramfs → AUR (as user) → user commands.

### Privilege model

The orchestrator runs as the user. Root steps go to **one helper session**
(the same binary in a hidden `helper` mode), started once through `pkexec` (polkit's dialog, or its text prompt on a
console); it reads steps as JSON lines and streams output back.
A FUSE mount is private to the user who mounted it, so from an AppImage the
binary is first copied to a private directory root can read. A version
handshake makes sure both sides speak the same protocol. The helper is as
powerful as sudo; see [APPLY.md](APPLY.md#trust-model).

### Safety

- Prefer **drop-in files** (`sysctl.d`, `makepkg.conf.d`, `udev/rules.d`,
  `tmpfiles.d`) with a `# Managed by Bindle` header over editing distro files.
- `pacman.conf` and `/etc/fstab` are the two in-place edits, both line-level and unit-tested. fstab has
  no drop-in directory, so drives mounted at startup go in one marked block ([DRIVES.md](DRIVES.md)).
- Before applying, snapshot every file that will be touched (XZram pattern) under
  `/var/lib/bindle/snapshots/<timestamp>/` with a manifest, so `bindle undo` can restore it. Details: [APPLY.md](APPLY.md).
- Respect existing setups: the zram builtin leaves an existing
  `zram-generator.conf` alone, defers to another `sysctl.d` file that already
  sets `vm.swappiness` (e.g. XZram's), and skips the zswap tmpfiles rule when
  `zswap.enabled=0` is already on the kernel command line. Config lookups follow
  each tool's search path (`/etc`, `/run`, `/usr/lib`, `*.conf.d` drop-ins).
- Drop-ins that must override a distro drop-in sort last (`makepkg.conf.d/zz-bindle*.conf`
  comes after Arch's `rust.conf`, which sets `RUSTFLAGS` outright).
- Tune only what changes something: the I/O scheduler module sets BFQ for
  rotational disks and nothing else (the kernel already picks `none` for NVMe and
  `mq-deadline` for SATA SSDs), so it is opt-in rather than a default.
- Risk levels (`safe` / `moderate` / `advanced`) are shown in the wizard; advanced modules are never in a profile.

### Relationship with XZram

Bindle's `zram` module covers the common case: zram-generator sized by RAM
tier (XZram's matrix), zstd, swap-tuned sysctls, zswap off. XZram stays the
tool for ongoing swap management (overflow swapfiles, priorities, snapshots,
doctor). The `xzram` module installs XZram (its CLI and Qt GUI, from its
release packages) for users who want it; it conflicts with `zram`.

## Starting point: an archinstall desktop install

Bindle targets the system archinstall leaves behind. It recovers archinstall's
choices (logged config, or rebuilt from pacman.log), shows them first, treats
them as done, and adds per-desktop essentials for the 21 desktops archinstall
offers. Details: [ARCHINSTALL.md](ARCHINSTALL.md).

## Detection (no root)

`/etc/os-release`, `/proc/{cpuinfo,meminfo,mounts,swaps,cmdline}`,
`/sys/class/drm/*/device/{vendor,device}` (GPU vendor; the NVIDIA device id picks
the driver: `nvidia-open` for Turing and newer, id ≥ 0x1e00; `nvidia-580xx` from
the AUR for Maxwell, Pascal and Volta, id ≥ 0x1340; nouveau for older cards),
`/sys/block/*/queue/rotational`, `lsblk --json` and `/etc/fstab` (other drives), `/sys/class/power_supply` (ignoring `scope=Device`
peripheral batteries) and `/sys/class/dmi/id/chassis_type`, `/sys/class/bluetooth`,
`systemd-detect-virt`, the `LoaderInfo` EFI variable (bootloader; `/boot` is often
root-only, so installed grub/limine/refind packages are a fallback hint),
`/etc/pacman.conf`, `pacman -Qq`, and `id -Gn` / `getent passwd` for the user.

## Frontends

1. **TUI (now)**: works in a bare TTY, over SSH, and in any terminal; `AppRun`
   opens a terminal emulator when the AppImage is double-clicked.
2. **GUI (Phase 3)**: `bindle-gui`, Rust + cxx-qt + Kirigami, following uQ's
   app conventions and theme tokens; it drives `bindle-run` like the TUI does.
   It needs a desktop session, so it complements the TUI and doesn't replace
   it. See [GUI.md](GUI.md).

## Growing past Arch

`Facts::is_arch_family` already accepts CachyOS, EndeavourOS and other
`ID_LIKE=arch` systems. Other families get their own `catalog/<family>/` plus a
package-manager backend in the executor; the planner and frontends are distro-agnostic.
