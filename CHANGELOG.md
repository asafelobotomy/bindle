# Changelog

## Unreleased

- NVIDIA's own driver is now the recommended one wherever it can be installed, with the open-source driver as the other choice. New module `nvidia-legacy` (NVIDIA driver for older cards): GTX 750/900/10-series and Titan V cards (Maxwell, Pascal, Volta) get `nvidia-580xx-dkms` and `nvidia-580xx-utils` from the AUR, with the kernel's headers and, with multilib, the 32-bit libraries. It is pre-selected on those cards; before, they got nouveau and a note. Untick it (or pick the open-source driver on `bindle install`'s Hardware page) to stay on nouveau. Kepler and older cards still get nouveau.
- `bindle install`'s Hardware page names the graphics choices plainly ("NVIDIA's own driver", "NVIDIA open-source driver (nouveau)") and says which one is recommended and why.
- New module `text-editor` (KWrite): a plain notepad, pre-selected with uQ and with Hyprland, which had no text editor to choose. With uQ it was installed silently as part of the desktop; it is now listed and can be unticked.
- New module `kyko` (drop-down terminal for uQ, by asafelobotomy), from its v0.0.2 release package. Offered, not pre-selected: 0.0.2 is an early preview without the drop-down window.
- `bindle install` shows one progress screen for the whole install instead of archinstall's scrolling output: the steps (disk, base system, boot loader, users and desktop, your apps and setup, with "Step 3 of 16" while Bindle sets the system up), the latest lines beside them, and Enter to restart at the end. A failed install stays on the failed step with the last lines. Everything printed is kept in `/var/log/bindle-install.log`, on the ISO and in the new system.
- `bindle install`: `p` on the Disk page is manual partitioning. It opens archinstall's own disk editor for that one step (keep another system, several disks, LVM, its own encryption) and returns to the wizard with the layout; `x` goes back to Bindle's whole-disk layout.
- `bindle install` no longer says "Arch is installed" when archinstall refused the configuration and installed nothing.
- `bindle install` has two more pages. Hardware lists what Bindle recommends for the detected machine (graphics driver, kernel, boot loader, zram swap, power profiles, Bluetooth, audio) and every row can be changed with ←/→; before, the recommendations were only shown and the kernel and boot loader were fixed. Apps lists everything the profile and desktop install, each one untickable with Space (`m` still opens the full bindle wizard, now from this page instead of Review).
- Setting up the new system before the reboot (`bindle install`, `bindle iso`) no longer stops at the first sudo prompt. archinstall's own `/etc/sudoers.d/00_<user>` rule asks for a password and sorted after Bindle's temporary grant, so it won; the grant is now `zz-bindle-install`.
- The uQ desktop also installs KWrite (`kate`), the screenshot tools uQ binds to Print (`grim`, `satty`, `slurp`, `wl-clipboard`) and `gnome-keyring`, which uQ's login screen now unlocks with your password.
- The uQ module runs `uqctl setup --apps`, so GTK, Qt (qt6ct and Kvantum) and the terminals follow uQ's theme from the first login. Before, uQ's own Settings and every Qt app stayed in the default light style under a dark uQ theme. Existing files are never overwritten; the step re-runs until `qt6ct.conf` loads uQ's colours, so installs from earlier releases pick it up at the next apply.
- PipeWire audio installs `rtkit`, so PipeWire and WirePlumber get realtime audio threads instead of logging "RTKit error: ServiceUnknown" at every login.
- Pre-Turing NVIDIA GPUs (GTX 10xx and older) no longer get nvidia-open from the live-ISO install, which left the card without a driver: the preset picks nouveau ("All open-source" on hybrid laptops). On a system that already has nvidia-open on such a card, the wizard now says to replace it with nvidia-580xx-dkms instead of keeping it.
- `bindle install` (also `bindleinstall`, and what `install.sh` now starts): the whole install from the live ISO in one wizard with the bindle wizard's look and keys. Network (Wi-Fi asked once, saved into the new system) → Desktop and profile with the hardware recommendations → Disk (Btrfs with Snapper or ext4, optional encryption) → You → Review (`m` changes the apps in the bindle wizard). archinstall runs unattended underneath; the first boot lands in the finished desktop. `install.sh iso` keeps the archinstall-menu flow.
- `bindle iso` now sets the new system up before the first reboot: at archinstall's last hook, Bindle applies the chosen profile inside the new system (`arch-chroot`, as its user, while the ISO is online), including the AUR helper and uQ. User commands that need a running session are saved and run once at first login. The first login lands in a finished desktop; Bindle only opens if a step didn't finish. `--setup-at-first-login` keeps the old behaviour.
- The root helper starts through `sudo -n` without a terminal when sudo needs no password, and enables services without starting them inside a chroot.
- Downloading the pinned AUR helper (paru-bin) retries twice and then falls back to Arch's GitHub copy of the AUR (same commits, so the pinned commit and checksum still apply). AUR installs through the helper get one more try, since the AUR often drops connections when busy.
- When Bindle fails in the terminal it opened (first login, or the window's "Continue in a terminal"), it no longer just closes: Enter tries again (finished steps are skipped), `s` opens a shell to fix the cause (e.g. `nmcli device wifi connect <name> --ask`), `q` closes. A fresh desktop may have no other way to open a terminal.
- `bindle iso` carries the ISO's Wi-Fi over: the networks you joined with `iwctl` are saved in the new system as NetworkManager connections (root-only keyfiles), so the first boot is online instead of failing at the first pacman step. Open and WPA2/WPA3-transition networks; 802.1X ones aren't carried.
- The `rustup` module sets the stable toolchain before the AUR builds, not after them. With the developer profile, every Rust AUR package (noti-fi, scdr) failed with "rustup could not choose a version of cargo to run". User commands can say `before_aur = true` (docs/CATALOG.md).
- Apply checks the internet connection before it changes anything, and stops with a plain message saying what's wrong (no connection, no DNS, or archlinux.org unreachable) and how to connect, instead of failing at step 1 with pacman's mirror errors. On a fresh install the live ISO's Wi-Fi doesn't carry over, so this was the first thing to go wrong.
- `makepkg-parallel` builds on half the cores (`MAKEFLAGS` and `CARGO_BUILD_JOBS`), not all of them: a full-core Rust build can run a laptop out of memory and get killed by earlyoom.
- The desktop window starts a polkit agent when it opens (polkit-kde, hyprpolkitagent, lxqt, polkit-gnome or mate, whichever is installed). archinstall's Hyprland installs one but never starts it, so on the first login Apply couldn't ask for the password and sent you to a terminal. On desktops that run their own agent the extra one fails to register and exits.
- New module `proton-pass-cli`: Proton Pass on the command line (`pass-cli`), from the AUR's `proton-pass-cli-bin` (Proton's release binary). The AUR package named `pass-cli` is an unrelated password manager that installs the same command, so `pass-cli login` failed after it was installed by mistake; the module replaces it (after you confirm, or with `--replace-conflicts`).
- New module `scdr-devbox`: scx_devbox, scdr's own sched_ext scheduler for development with LLM agent apps (the focused window and short tool calls first, builds wait; scdr hands it the focused app). From the AUR's `scx-devbox`, split from `scdr` from its next release. Conflicts with `scdr-sched-ext` and `scx-schedulers`, so only one thing starts a scheduler.

## 0.1.8 — 2026-11-11

- uQ hand-off (uQ Phase 5): while the first-login wizard is open in a uQ session, Bindle holds `$XDG_RUNTIME_DIR/uq/welcome.defer` so uQ's Welcome waits; when the wizard ends it removes the file and runs `uqctl welcome`. A crash still lets Welcome open (uQ ignores a defer file older than 30 minutes).
- The defer file is created as soon as the login hook starts, before Bindle waits for the desktop's own welcome apps, so uQ (which decides about 20 s after login) never opens Welcome on top of the wizard.
- Closing the terminal window the first-login wizard runs in (SIGHUP), logging out (SIGTERM) or an interrupt (SIGINT) now still removes the defer file and runs `uqctl welcome`, so uQ's Welcome opens in that session instead of at the next login. The same when the wizard's terminal process returns early (already done, or not a regular user session) after taking the hold over.
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
