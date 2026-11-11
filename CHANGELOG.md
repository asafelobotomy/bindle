# Changelog

## Unreleased

- uQ hand-off (uQ Phase 5): while the first-login wizard is open in a uQ session, Bindle holds `$XDG_RUNTIME_DIR/uq/welcome.defer` so uQ's Welcome waits; when the wizard ends it removes the file and runs `uqctl welcome`. A crash still lets Welcome open (uQ ignores a defer file older than 30 minutes).
- The `uq` module installs `greetd` and runs `uqctl greeter enable` as root, so uQ's own login screen replaces the one archinstall set up from the next boot (`run0 uqctl greeter disable` switches back). Needs a uq release with the login screen on the AUR.
- `bindle status [--json]`: the selection Bindle last applied for you, when, and the steps applying it again would run.
- uQ Settings gets a "Re-run setup" page (`uq/pages/bindle/`, installed by `bindle first-login install` with `/usr/lib/bindle/bindle` linking to Bindle): shows the profile and when it last ran, "Pick apps again" opens Bindle (`bindle open --detach`), and "Check for missing pieces" lists what isn't in place.

## 0.1.7 — 2026-11-10

- New module `uq-dev` (uQ developer setup, developer profile, only with uQ): clones uQ and its apps into `~/Projects` over HTTPS, installs the build and test tools, and points to `uq-dev` (session, run, promote, rollback) from the uq package.
- uQ is now the recommended desktop: `bindle iso` pre-selects it. `uq` is on the AUR, and the fresh-VM install of uQ with scdr, crumbs and noti-fi passes.
- New modules `appofholding` (AppImage manager, from its v0.5.0 release package) and `scale_tail` (Tailscale tray, from its v0.5.1 AppImage; pulls in `tailscale`).
- uQ's own apps: new modules `scdr` (process priorities; replaces ananicy-cpp and system76-scheduler after importing their rules), `scdr-sched-ext` (scdr picks the sched_ext scheduler; conflicts with `scx-schedulers`), `noti-fi` (notifications; replaces mako, dunst, swaync) and `crumbs` (uninstaller and cleaner). `bindle iso` pre-selects scdr, crumbs and noti-fi with uQ.
- Modules that install from the AUR or a release enable their units and run their root commands after that install. `replace_after_install` removes their `package_conflicts` after it too.
- `when` takes `installed:<package>`.

## 0.1.6 — 2026-11-09

- Logo: the bundle is a knotted, lumpy cloth hanging from a tie round the
  stick, and the stick's far end is hidden behind it.
- The icon everywhere: PNG sizes (16–512) next to the SVG for first-login
  installs and the AppImage, a PNG `.DirIcon`, the desktop window's own icon
  (X11 and window lists), and the README.

## 0.1.5 — 2026-11-09

- New logo: a traveller in a fedora, seen from behind, with the bindle over
  the shoulder and its bag in the foreground.

## 0.1.4 — 2026-11-09

- The Bindle icon is flipped vertically.
- `bindle undo` marks a snapshot undone once it is fully restored, so running
  it again steps back to the apply before instead of repeating the newest one.
  After an apply was re-run, the first run's file changes were only reachable
  with `bindle undo <id>`.
- `install.sh`: a failed download no longer ends in `dir: unbound variable`
  from the cleanup trap; the temporary directory is removed.
- `bindle first-login snippet` warns when no `--sha256` is given.

## 0.1.3 — 2026-11-08

- The desktop window opens at first login on graphical sessions (falling back
  to the terminal wizard when it can't start), and from the app menu
  (`bindle open`). Releases ship `bindle-gui-x86_64`, signed with the rest;
  `install.sh` verifies it when a release lists it; `bindle iso` installs it
  with `kirigami` and `qt6-declarative` for graphical profiles.
- Desktop wizard, second part: Apply and Done screens. The window runs the
  reviewed plan with live per-step status and output, asks for the password
  through pkexec (a new polkit action, installed by `first-login install`,
  explains why), offers a terminal when no polkit agent is running, and on
  success saves the selection like `bindle apply`. `bindle-gui --sandbox`
  applies in a sandbox as the CLI does.
- Desktop wizard, first part (`bindle-gui`, Qt 6 + Kirigami): Welcome,
  Profile, Choose (with search and module details) and Review screens, themed
  from uQ's tokens and following uQ theme changes live; the desktop's Qt
  theme elsewhere. Applying from the window comes next. `make gui-shots`
  renders every screen per uQ theme.
- Internals: the apply engine (orchestrator, root helper, protocol,
  snapshots, downloads, selection files and shared text) moved into a new
  `bindle-run` crate, which reports progress through a `Progress` trait. The
  terminal output is unchanged. This prepares the desktop wizard planned in
  docs/GUI.md (cxx-qt + Kirigami, themed by uQ).
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
