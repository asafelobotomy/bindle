# Changelog

## 0.1.1 — 2026-11-08

Fixes from the November 2026 audit.

### Safer installs
- A non-UTF-8 byte in pacman or makepkg output no longer kills the
  transaction mid-way.
- An installed NVIDIA driver (Arch, legacy branches, or a derivative's
  kernel-specific package) is kept instead of replaced by a conflicting one.
- Modules that conflict with installed packages (rustup vs rust, PipeWire vs
  PulseAudio/JACK, TLP vs power-profiles-daemon, Node.js LTS vs nodejs) offer
  to replace them after you confirm; with `--yes` they're skipped unless
  `--replace-conflicts` is given.
- On Arch derivatives, Arch-only modules (reflector, linux-zen) are skipped and
  NVIDIA drivers are left to the distro's own tool.

### Apply and undo
- `bindle undo` reverts a failed apply together with the re-run that finished
  it, and keeps going past files it can't restore (reporting them).
- Directories created by apply are removed on undo.
- Group changes go to the user running Bindle, never `$SUDO_USER`.
- Only one apply or undo runs at a time; Ctrl-C no longer kills the root
  helper mid-step.
- pacman.conf edits leave prose comments alone, keep CRLF, and are checked
  with `pacman-conf` before they're written.
- Snapper setup on an `@.snapshots` layout resumes cleanly after a failure.

### AUR
- The paru bootstrap builds from a pinned, checksummed AUR commit and
  installs root-owned copies; `-debug` packages are skipped.
- Interactive applies let paru/yay show PKGBUILDs for review.

### Live ISO
- Releases are GPG-signed (`SHA256SUMS` + `SHA256SUMS.sig`); `install.sh`
  verifies the signature against its embedded key and refuses anything else,
  pins "latest" to one tag, requires HTTPS, and validates its environment.
- `bindle iso` checks three signals that it's on the Arch live ISO.
- `--silent` requires and shows the target disks; the merged config is deleted
  afterwards.
- A user config's repositories no longer drop the gaming profile's multilib.

### First login
- The launch lock follows its owner process, so a long wizard isn't doubled.
- Bindle offers to remove its first-login hooks once every user is done.
- Install detection survives logs with invalid UTF-8.

### Project
- CI: actions pinned to SHAs, `cargo deny`, an MSRV job, unit and AUR name
  checks, a 400-line file rule; releases build with a pinned toolchain.
- Stricter catalog validation; CLI and helper-protocol integration tests.

## 0.1.0

First release.
