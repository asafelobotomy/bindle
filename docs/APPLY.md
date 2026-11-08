# Applying changes

```bash
bindle apply --profile desktop            # shows the plan, asks, applies
bindle apply --from ~/.config/bindle/selection.toml --yes
bindle undo --list
bindle undo                               # latest snapshot
```

In the wizard, press **a** on the Review screen. Bindle leaves the TUI, asks
for confirmation and your password in the terminal, then streams progress.

## How it runs

1. **Plan**: same pure resolver as `bindle plan`; anything already done is dropped.
2. **Root helper**: Bindle runs as you and refuses to run as root. It starts one helper
   (`bindle helper`, hidden) through `sudo` on a terminal (credentials validated
   first with `sudo -v`), or `pkexec` in a desktop session without a terminal. The
   two speak JSON lines; the helper greets with its version, which must match.
   From an AppImage, the binary is first copied to a private `0700` directory in
   `$XDG_RUNTIME_DIR` (or `~/.cache/bindle`, never a shared `/tmp` path), because
   root cannot read a user's FUSE mount.
3. **Snapshot**: before any change, the helper records every file the plan will write
   in `/var/lib/bindle/snapshots/<id>/`. Copies of the originals sit in a root-only `files/`;
   `manifest.json` (paths, units, packages; world-readable) drives `bindle undo`.
   If the snapshot fails, nothing has changed. A file that didn't exist yet but
   appears during the apply (shipped by a package installed in an earlier step)
   is captured just before Bindle overwrites it, so undo restores the package's
   version rather than deleting it. Owners and modes are recorded too.
4. **Steps**, in plan order. Root steps go to the helper; AUR builds (`makepkg` as you,
   then `pacman -U` via the helper), `paru`/`yay` installs and user commands run as you.
5. **Stop on first failure** with the log path and the undo command. Every step is
   idempotent, so fixing the cause and re-running continues where it stopped.
6. **Finish**: reboot hint (kernel, microcode, drivers, initramfs) or a relogin hint (groups),
   and the selection saved to `~/.config/bindle/selection.toml` for replay.

| Step | Runs as | Command |
|---|---|---|
| edit pacman.conf | root | line-level edit, atomic replace |
| replace conflicts | root | `pacman -Rdd --noconfirm -- …` (only after you confirm; the next step installs the replacements) |
| install packages | root | `pacman -Syu --needed --noconfirm -- …` (a full system upgrade) |
| write file | root | atomic write: exclusive temp file created with the final mode, keeps the owner, refuses to replace a symlink |
| enable units | root | `systemctl enable --now -- …` |
| groups | root | `usermod -aG … <you>` |
| root command | root | as declared in the catalog |
| initramfs | root | `mkinitcpio -P` |
| AUR helper bootstrap | you, then root | `git clone` at a pinned commit + PKGBUILD checksum + `makepkg`, then `pacman -U` of root-owned copies |
| AUR packages | you | `paru -S --needed -- …` (you review PKGBUILDs); with `--yes`: `--noconfirm --skipreview` |
| release assets | you, then root | `curl --proto =https` + sha256 pin; packages via `pacman -U` (root-owned copies), AppImages to `~/.local/bin` |
| user command | you | as declared |

Logs: `/var/log/bindle/apply-<id>.log` (and `undo-<id>.log`), root-only (`0600`).
They record commands and their output; written files appear as path, mode and
size, not their contents. Directories Bindle creates are `0755` whatever the
umask, so `bindle undo` can list snapshots as you.

## Undo

`bindle undo [ID]` restores each snapshotted file byte-for-byte (removing ones
that didn't exist before, then the directories apply created for them), with
their original owner and mode, and stops and
disables (`systemctl disable --now`) the units that apply enabled. Units that
were already enabled are never in the plan, so they're never disabled. Packages
are left installed: removing them automatically is too risky. Undo prints the
`pacman -Rns` command for the ones still present. Group membership and catalog
commands (for example `ufw enable`, `systemctl mask`, `snapper create-config`)
aren't reversible in general; undo lists them under "Not reverted", as are
packages removed to make way for replacements (with the `pacman -S` to get
them back).

Without an ID, undo reverts the latest apply **and** any failed applies it
resumed (a failed apply's snapshot stays "incomplete"), newest first, so the
system ends up as it was before the first attempt. A file that can't be
restored doesn't stop the rest; undo lists what it couldn't do and exits
non-zero. Only one apply or undo runs at a time (`/var/lib/bindle/lock`).

## Trust model

The helper executes privileged plan steps the orchestrator sends, so it is
exactly as powerful as `sudo` for the user who authenticated. Code already
running as that user could equally hijack `sudo` itself, so the helper doesn't
pretend otherwise. It does refuse non-privileged steps, unsafe paths (relative
or with `..`), invalid user/group/package/unit names (including ones starting
with `-`; names are also passed after `--`), and snapshot ids that aren't
timestamps.

The AUR helper bootstrap is the one step where code from outside the Arch
repos runs as root (its package's install scriptlet, via `pacman -U`). Bindle
builds `paru-bin` only from the AUR commit pinned in
`crates/bindle-core/src/builtins/mod.rs` (`AUR_HELPER_COMMIT`) and refuses a
PKGBUILD whose sha256 differs from `AUR_HELPER_PKGBUILD_SHA256`; that PKGBUILD's
own `sha256sums` pin the release binary. To bump: read the new PKGBUILD, then
update both constants together. Other AUR packages go through paru/yay as you,
which show the PKGBUILDs for review unless you passed `--yes`.

## Testing without root

`--sandbox <dir>` (hidden, on `apply`, `undo` and `wizard`) runs the real
helper process unprivileged against `<dir>` as the filesystem root: files,
snapshots and logs are written there, and commands are recorded as
`would run: …` instead of executed. Seed it with `etc/pacman.conf` first.
Detection still reads the real system.

This exercises the whole pipeline except real package installs and
`systemctl`. Before a release, run a real apply in an Arch VM.
