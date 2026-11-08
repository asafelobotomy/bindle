# Changelog

## Unreleased

- Clearer wizard and output:
  - Shorter category names, modules grouped logically within each category,
    and consistent "What it does (tool)" names for system modules.
  - The Pick screen's detail pane lists what a module installs, enables,
    writes and runs (hardware modules are expanded for this machine), and
    shows the category's description while the category list is focused.
  - Reasons a module doesn't apply read as English ("needs Arch Linux",
    "not for a virtual machine") instead of raw tags.
  - The review leads with what changes (replacements, skips) and the steps;
    long lines wrap under their step. The module list moves to the end.
  - Apply prints one line per step instead of three; undo says up front that
    packages stay installed and confirms when it's done.
  - The install summary hides fields Bindle couldn't determine.
  - The ISO pre-install screen fits an 80-column console.
- New **Internet** category. Firefox, Thunderbird, Signal,
  Telegram, Element, qBittorrent and the download apps move here (module ids
  are unchanged). New:
  - Browsers: Chromium, Brave, Brave Origin, Vivaldi, Zen, Google Chrome.
  - Proton: Mail, Mail Bridge, VPN, Pass, Authenticator, Drive CLI.
  - Syncthing (as a user service), Nextcloud client, Tailscale,
    qBittorrent Enhanced Edition (alternative to qBittorrent).
- New **AI tools** category: Claude Code, Claude Desktop, Codex CLI, Gemini
  CLI, Grok Build, GitHub Copilot CLI, opencode, Cursor, LM Studio, and Ollama
  with CUDA or ROCm backends.
- More opt-in modules:
  - Desktop: Nerd Fonts (Developer profile), Yakuake (Plasma), Obsidian, Spotify.
  - Development: modern CLI tools (Developer profile), fish, Zsh, kitty,
    Alacritty, VSCodium, virt-manager with QEMU/libvirt.
  - Hardware: scanning (SANE + airscan with Skanpage or Document Scanner),
    fingerprint login, Solaar, Piper, OpenRGB, Android tools.
  - Gaming: Prism Launcher.
  - Security: Timeshift (alternative to snapper), OpenSnitch.
- Catalog review (November 2026):
  - New default `audio-firmware` (sof-firmware etc.).
  - Network printer discovery.
  - NVIDIA VA-API decoding.
  - Better reflector settings.
- New opt-in modules:
  - System: man pages and pkgfile, thermald, earlyoom, sched_ext schedulers,
    grub-btrfs, Btrfs scrub, sbctl.
  - Desktop: LibreOffice, media players, password managers, mail and chat,
    backups, qBittorrent, Flathub in Discover / GNOME Software.
  - Development: Go, Java 21, .NET, Zig, mise, Bun/Deno/pnpm, Distrobox,
    Code - OSS, Zed, Helix, Ghostty, Jujutsu, git and C/C++ tools.
  - Gaming: gamescope, Protontricks, ProtonUp-Qt, GOverlay, LACT, Discord or
    Vesktop, Bottles, emulators, Moonlight. Steam adds `steam-devices`.
- New **Creative** category:
  - GIMP, Krita, Inkscape, darktable, RawTherapee, digiKam, Kdenlive, OBS
    Studio, Audacity, HandBrake, Blender.
  - The ArtCraft suite (PhotoCraft, LightCraft, FilmCraft, VectorCraft,
    EffectCraft, DesignCraft, PdfCraft, WordCraft, DeckCraft, GridCraft),
    marked alpha and installed from pinned GitHub releases.

- asafelobotomy's apps as opt-in modules: XZram (Performance, alternative to
  the zram module), oysterAV (Security, AUR), romforge and ReShadeLinux
  (Gaming), emobie (Desktop), AV Morning Star, hauler and qbit Plugin Manager
  (Internet).
- Modules can install pinned, checksummed GitHub release assets
  (`[[module.release]]`): Arch packages via `pacman -U`, AppImages to
  `~/.local/bin` with a desktop entry.

## 0.1.2 — 2026-11-08

(0.1.1 was tagged but never released: its release build failed.)

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
