# Changelog

## 0.1.46 — 2026-11-26

- **Snapshots by hand: `bindle snapshot`.** List the system's snapper snapshots, take one now (`bindle snapshot create "Before the party"`), delete old ones, and see or change how many automatic snapshots snapper keeps (`bindle snapshot settings`, `bindle snapshot set TIMELINE_LIMIT_DAILY 7`). Changes ask for the password once; only the automatic-snapshot settings can be changed this way. uQ Settings' new Restore page uses these.
- **`bindle recovery --status`** prints, as JSON, which of Roll back and Factory Restore this computer allows and why not, whether the installer is kept, and whether snapper is set up, so uQ Settings can show only what works here.

## 0.1.45 — 2026-11-26

- **bindleUP's bar icon shows on Bindle installed from its release.** The icon had the same `[requires] bins = ["bindle"]` condition that hid uQ Settings' Bindle page until 0.1.43: uQ looks it up in `/usr/bin` only, so with Bindle in `/usr/local/bin` the icon stayed hidden (`uqctl integrations list`: "hidden: needs /usr/bin/bindle"). `first-login install` and `refresh` now write the icon without it, like the page. Updating to 0.1.45 with bindleUP fixes it by itself, since the new Bindle writes its uQ files again.

## 0.1.44 — 2026-11-25

- **A failed AUR build no longer hides what happened.** paru builds AUR packages one after another and fails the whole run when any one of them fails, so a single broken build made bindleUP's AUR stage say only "`paru -Sua …` failed (exit status: 1)", with paru's own error long scrolled past. The stage's result now names the packages that didn't upgrade (`not upgraded: kyko 0.2.2-1 → 1.3.2-1`), and in the update window and uQ Settings, where paru's output comes line by line, it adds paru's error and makepkg's (`packages failed to build: … (A failure occurred in check().)`).
- **What did upgrade still counts.** The packages paru upgraded before the failure used to count as not upgraded, so the uQ stage said "no uQ update" after uQ itself had just been upgraded from the AUR, and skipped `uqctl setup --apps`; the reboot check missed them too. bindleUP now compares the installed versions after a failed stage and treats those packages as upgraded.

## 0.1.43 — 2026-11-24

- **uQ Settings' Bindle page shows on Bindle installed from its release.** The page asked uQ to show it only while `/usr/bin/bindle` exists, which is true for the AUR package but never for Bindle in `/usr/local/bin` (the live-ISO and release install), so uQ kept it hidden. `first-login install` now writes the page without that condition (`first-login remove --purge` takes it away), and also installs Bindle's problem codes (`/usr/share/uq/diag/bindle.toml`), so `uqctl explain BINDLE-…` and Settings › Problems know them there too.
- **The page and bar icon follow Bindle's updates.** bindleup replaced only the programs, so the page, bindleUP's bar icon, the launcher and the password message stayed as the first install wrote them. A new `bindle first-login refresh` (root) writes them again without touching the login hooks, and bindleup runs it after Bindle updates itself. On a machine updated to 0.1.43 by an older Bindle, run `sudo bindle first-login refresh` once.
- **What's new offers uQview** to setups made before Bindle 0.1.31, when it joined uQ's apps. It becomes the app for pictures only if you haven't chosen one.

## 0.1.42 — 2026-11-23

- **One source for uQ's shared values.** Bindle now vendors uQ's uq-consts 1.0.0: the window's `Theme` is uQ's `UqThemeBase.qml` (same colours, shape, type and motion as every uQ app, with Bindle's desktop-palette fallbacks off uQ), and bindle's uQ settings page and widget folders come from `uq_consts.rs`, with a test that they match. uq-theme moves to 1.1.0, so tokens' `motion` block (uQ's Reduce motion setting) is honoured. CI checks every vendored kit with uQ's single `scripts/check-uq-native.sh` and `uq/native.toml`, replacing the two separate check scripts.

## 0.1.41 — 2026-11-22

- **Zsh comes with its plugins.** The Zsh module now also installs zsh-autosuggestions, zsh-syntax-highlighting and zsh-history-substring-search, which kyko 1.1 loads (with a saved history, a completion menu and Up/Down history search) when your own .zshrc doesn't. A new optional **fzf-tab** module (Terminals & shells, from the AUR) turns zsh's completion menu into a fuzzy search with previews. What's new offers it to Zsh users.

## 0.1.40 — 2026-11-22

- **A failed update step shows on uQ's Problems page.** When a bindleUP stage fails (the system packages, the AUR, Flatpak…), in a terminal, the update window or uQ Settings, bindleUP writes it to your journal as `BINDLE-004` under the name `bindle`. uQ 0.3.36 reads it there, so Settings › Problems and `uqctl log` list it, and `uqctl explain BINDLE-004` says what to do. Older uQ versions ignore it.

## 0.1.39 — 2026-11-22

- **bindleUP stops rebuilding the same AUR packages on every run.** rebuild-detector flags a package when one of its files links a library that isn't there; for a package that ships prebuilt files (codeql, nvidia-580xx-utils with its optional GTK 2 panel) a rebuild changes nothing, so every bindleUP run rebuilt them again, and rebuilding the driver package regenerated the boot images each time. After the Rebuilds stage bindleUP now looks again and remembers each package the rebuild didn't clear, at that version (`~/.local/state/bindle/rebuild-stuck.json`). That version isn't rebuilt again; it is listed under Rebuilds instead, with `checkrebuild -v` to see why. A new version of the package is tried again.
- **One snapshot pair per bindleUP run.** With snap-pac installed, a run used to leave a pre and post snapshot for every pacman call (each AUR install and rebuild too): about ten per run, so snapper's number limit kept only a few runs of history. bindleUP now takes one "bindleup" pre snapshot before the packages stage and its post after the last stage, and snap-pac skips its own snapshots for the pacman calls in between (`SNAP_PAC_SKIP`). If the run is cut short, the root helper still takes the post as it exits. Without snap-pac the run gets the same pair in place of the single snapshot it had.
- **The last checks see what the rebuilds changed.** After rebuilds ran, uQ, config files, orphans, services and Reboot are planned again from a fresh look, so a rebuilt DKMS module or a regenerated boot image is checked before Reboot says whether restarting is safe.

## 0.1.38 — 2026-11-22

- **Bindle's problems show in uQ.** Bindle now uses uQ's shared uq-diag module (`crates/bindle-core/src/uq_diag.rs`, uq-diag 1.0.1). A crash of `bindle`, bindleUP or the Bindle window leaves a crash record that uQ 0.3.35 lists on Settings › Problems with a report to copy. Bindle's warnings and errors have codes (`BINDLE-001` the window couldn't open, `BINDLE-002` a background task such as the daily update check failed, `BINDLE-003` a setting in upgrade.toml was ignored), explained in plain words by `uqctl explain` and the Problems page from `/usr/share/uq/diag/bindle.toml`. Under systemd (the update-check timer) Bindle writes uQ's log format so the journal files warnings as warnings; in a terminal its messages are unchanged. CI checks the vendored copy matches the release, and a test keeps the codes and the list in step.

## 0.1.37 — 2026-11-22

- **kyko is uQ's terminal.** kyko (from the AUR) is back in the catalog under Terminals & shells and is pre-selected with uQ's own apps in place of kitty, which leaves uQ's essentials. uQ opens kyko on SUPER+Return and falls back to kitty where kyko is not installed; a kitty you already have stays. What's new offers it to existing uQ installs.

## 0.1.36 — 2026-11-22

- **Bindle, bindleUP and Recovery move like the desktop.** Hover and press colours fade in 100 ms instead of snapping, and fades, slides and toasts use uQ's shell speeds (100, 140 and 220 ms) with an ease-out curve instead of a mix of 100 to 200 ms and linear easing. The speeds are `Theme.motionFast`, `motionBase`, `motionSlow` and `Theme.easing`, matching uQ's `shell/core/Motion.qml`.

## 0.1.35 — 2026-11-21

- **uQbit, uQ's torrent app.** A new `uqbit` module (Downloads, from the AUR) runs qBittorrent Enhanced Edition's headless engine (qbittorrent-enhanced-nox, replacing a plain qbittorrent-nox) in the background with a window that follows the uQ theme; no qBittorrent window is installed. uQ pre-selects it in place of qBittorrent. It becomes the app for magnet links and .torrent files unless something other than qBittorrent already has them, and can sit beside the qBittorrent modules. It also installs Python, which qBittorrent's search plugins need. What's new offers it on upgrade.

## 0.1.34 — 2026-11-21

- **The windows follow uQ's theme without polling.** Bindle, bindleUP and Recovery used to check uQ's theme file once a second; they now use uQ's shared uq-theme module (`crates/bindle-gui/src/uq_theme.rs`, uq-theme 1.0.0), which watches the file and also catches uQ swapping the whole theme folder.
- AppStream metadata (`io.github.asafelobotomy.bindle.metainfo.xml`), so software centres show Bindle; the desktop entry lists one main category (`System;`).

## 0.1.33 — 2026-11-21

- **One update window at a time.** Clicking uQ's Bindle bar icon again (or running `bindleup --gui` twice) opened a second update window that could start a second update while the first ran. A second start now brings the open update window forward; the Recovery window works the same way.
- **uQ developer setup** also clones uQview, kyko and AV Morning Star into `~/Projects`.

## 0.1.32 — 2026-11-20

- **AUR updates finish in the update window again.** When the window updated AUR packages, paru's password-free `sudo` started a second Bindle window instead of passing the pacman command on, so the AUR step sat waiting for that window, and its Qt messages ("Failed to register with host portal", "QWaylandTextInputv3::disableSurface") filled the window's output. paru's `sudo` now runs `bindle` itself, and `bindle-gui` never opens a window when started that way.

## 0.1.31 — 2026-11-20

- **uQview is uQ's picture viewer.** A new `uqview` module (from the AUR) is one of uQ's apps, pre-selected on first login: it views pictures in uQ's theme, marks up screenshots and sets a picture as the wallpaper, and becomes the app for pictures unless you already chose one. On uQ the everyday apps no longer include Gwenview. A Gwenview Bindle installed earlier stays; remove it with crumBS if you don't want it.

## 0.1.30 — 2026-11-20

- **reelo is uQ's media player.** uQ pre-selects reelo (from the AUR) instead of VLC and mpv, and makes it the default for video and music unless another player was chosen. What's new offers it to uQ users upgrading. Other desktops keep the VLC and mpv module.

## 0.1.29 — 2026-11-20

- **No system-config-printer on uQ.** uQ's Settings now has its own Printers page, so the printer settings window is its own module (Printing & scanning), installed with Printing only on desktops without one: not on uQ, Plasma or GNOME. Systems that already have it keep it. The Printers page comes with uQ 0.3.25.

## 0.1.28 — 2026-11-20

- **bindleUP in uQ's bar.** A bindleUP icon in the Status island shows how many updates are ready; click it for the list, Update… and Check now (a middle click opens the update window). A new preference, "Show the bindleUP icon" (Preferences → uQ bar in uQ Settings, `bar_icon` in upgrade.toml), shows it only when updates are ready (the default) or always. Every check now saves what it found, and `bindle updates` reads it without the network. Hiding the icon needs uQ 0.3.22; an older uQ always shows it.

## 0.1.27 — 2026-11-20

- **asafelobotomy's apps come from the AUR.** emobie, appOFholding, hauler, AV Morning Star, romforge (`romforge-bin`), scale_tail and XZram are installed from the AUR like uQ, noti-fi, scdr and crumBS already were, so the AUR stage of bindleUP keeps them up to date and Bindle no longer pins their GitHub releases. appOFholding, hauler and XZram keep their package names, so the copies Bindle installed update in place. What's new offers the change for each; the emobie, romforge, AV Morning Star and scale_tail AppImages Bindle put in `~/.local/bin` are removed once the package is in (only while Bindle's own menu entry for them is still there).

## 0.1.26 — 2026-11-19

- **bindleUP rebuilds AUR packages by itself.** Packages a library update broke are rebuilt with paru (no review, no question), named in the Rebuilds row, in the terminal, the window and uQ Settings alike; the window and uQ Settings now update the AUR too, instead of leaving it to a terminal.
- **No false "don't restart yet" on unified kernel images.** A system whose mkinitcpio presets write UKIs (or with UKIs on the EFI partition) keeps no `initramfs-*.img` in /boot, so that is no longer reported missing. When boot images or DKMS modules really are missing, they are built without a question, and the row says "builds the missing … then restart".
- **emobie starts at login again.** Pinned to emobie 0.9.2, which rewrites a login entry that pointed at a file name an update had removed, and to appOFholding 0.12.2, whose updates now move an app's URL handlers and login entries to its new file name.

## 0.1.25 — 2026-11-19

- **No more CopyQ error popups on uQ.** uQ keeps its own clipboard history (uqlip, Super + Shift + V), so Bindle no longer sets up CopyQ there. A CopyQ it installed earlier is offered for clean-up in What's new and `bindle plan`, which ends its "Command copyq onClipboardChanged, exit code 1" notifications.

## 0.1.24 — 2026-11-19

- **bindleUP asks once, then runs by itself.** At a terminal it first lists what it would update (packages, AUR, apps, Flatpak, rebuilds, firmware, orphan clean-up), all switched on: Enter starts, numbers switch items off. Questions that default to "no" (an Arch news post asking for a manual step, upgrading on battery) are asked in the same place. After that comes the one password and no more y/n, no pacman prompts and no AUR build-file review; problems are listed at the end. An unattended run now waits when the snapshot failed, instead of upgrading without one. `--yes` still skips the list.
- **Newer romforge.** Pinned to romforge 0.20.0, which also opens ROMs from your file manager and brings the command line and app closer together. Its uQ window rules match DuckStation, Azahar and PPSSPP by the app IDs they set (plus a second rule for six more emulators), and the AppImage can add its games to uQ search from Settings (your own files, removable).

## 0.1.23 — 2026-11-18

- **bindleUP's AUR bridge trusts fewer package files.** `pacman -U` through the one-password bridge now only takes a plain package file (not a link) owned by you or root that group and others can't write, so a package dropped in `/tmp` by another account is refused.

## 0.1.22 — 2026-11-18

- **Faster `list`, `detect`, `plan` and `status`.** `bindle list` no longer inspects the machine, so it prints at once (`bindle list --here` adds the drive modules and the "n/a here" marks). The other commands run their system probes (pacman, lsblk, id, getent, systemd-detect-virt) side by side, read package groups (`pacman -Sgg`, the slowest probe) only when something asks for them, and keep that answer in `~/.cache/bindle/package-groups` until a sync database changes.

## 0.1.21 — 2026-11-18

- **Newer appOFholding.** Pinned to appOFholding 0.12.1, which only checks uQ's theme while its window is showing.

## 0.1.20 — 2026-11-18

- **bindleUP asks for the password once.** The root helper now starts before the first stage (when anything needs it), and what used to ask on its own goes through it: the AUR helper's `sudo pacman` calls (a per-run socket, `pacman` only, package files only for `-U`), Flatpak's system installation and fwupd's updates. Start it, enter the password, walk away (`--yes` also answers pacman's and the AUR's own questions). A run that replaces Bindle itself still asks a second time, in the new Bindle.
- **bindleUP starts faster.** Its look at the system ran every check one after the other; the network ones (Bindle's release, the news, `checkupdates`, the AUR, Flatpak, fwupd) and the local ones now run side by side, as do the closing looks (`checkrebuild`, `dkms`, orphans, failed units). AUR builds use all cores (`MAKEFLAGS=-jN`) when nothing else sets it.
- **A short "What's new in Bindle X.Y.Z" after Bindle updates.** The first time a newer Bindle opens (the wizard, the update window after bindleUP replaced Bindle, or the terminal), a popup lists up to four highlights of that release, taken from this changelog, with "Open What's new" for the per-item review and "Dismiss". It shows once per version, never on a fresh install, and bindleUP's notification carries the same summary.

## 0.1.19 — 2026-11-18

- **oysterAV installs chkrootkit too**, next to ClamAV and rkhunter, so its scans have all three rootkit and malware scanners.
- **Newer romforge.** Pinned to romforge 0.18.1.

## 0.1.18 — 2026-11-17

- **Newer hauler, XZram and appOFholding.** Pinned to hauler 0.2.1, XZram 0.7.1 and appOFholding 0.12.0; all three now follow the uQ theme.

## 0.1.17 — 2026-11-17

- **oysterAV installs by default on desktop, gaming and developer profiles**, along with ClamAV and rkhunter, and starts the signature updater (`freshclam`), so its first scan has something to check against. A boot-guard status recipe was added.
- **Newer romforge and AV Morning Star.** Pinned to romforge 0.18.0 and AV Morning Star 0.4.3, which follows the uQ theme.

## 0.1.16 — 2026-11-17

- **A module Bindle retired no longer breaks a selection saved before.** Selections saved by Bindle 0.1.8 to 0.1.10 list the ArtCraft apps (pdfcraft, photocraft, wordcraft…) that 0.1.11 retired; every command that read the file (`bindle whats-new`, `bindle status`, `apply --from`, bindleUP's setup check) stopped with "unknown module `pdfcraft`" or, in bindleUP, said "nothing from Bindle to compare". Retired ids are now skipped; a name that never existed is still an error.

## 0.1.15 — 2026-11-17

- **bindleUP no longer holds the restart for the kernel copy kernel-modules-hook keeps.** After a kernel upgrade, the running kernel's modules stay as a copy no package owns; bindleUP counted it as an installed kernel, said "don't restart yet: nvidia isn't built for" it and offered a build that can't work (its headers are gone). It now ignores that copy for DKMS and boot-image checks, and treats it as the sign the running kernel was replaced, so the reboot line says so.

## 0.1.14 — 2026-11-17

- **bindleUP checks unified kernel images.** The Reboot stage reads the kernel inside each UKI on the EFI partition and flags one that holds a kernel which is no longer installed (the leftover "previous" image that broke a boot), or an installed kernel no image holds ("don't restart yet", then `mkinitcpio -P`). Same rule as the boot guard; it works without `objcopy` and through the root helper when the partition is root-only.

## 0.1.13 — 2026-11-17

- **Newer appOFholding and emobie.** appOFholding 0.11.0 can add an AppImage from a link and go back to an earlier version from uQ Settings; emobie 0.9.1 (0.9.0 never shipped) brings its uQ Settings page and command line.
- **Newer hauler, appOFholding and romforge.** Pinned to hauler 0.2.0, appOFholding 0.10.0 and romforge 0.17.0, which can check what they download or install with oysterAV (off by default). scale_tail is now 0.7.0 and XZram 0.7.0.
- **The Bindle page's Setup tab sums up what is missing** instead of listing every step: "Install 82 packages", "Write 23 settings files", "Turn on 10 services and timers", "Run 16 setup commands", each with the names under it. "Show every step" opens the full list, and a script shows as its first line. `bindle status --json` gained a `groups` list for it.
- **Boot guard** (new module, on by default): a pacman hook refuses a kernel or mkinitcpio change while `/boot` (or `/efi`) is listed in fstab but not mounted, and another warns after a kernel change when a unified kernel image on the boot partition holds a kernel that is no longer installed, or an installed kernel has no image. A leftover image (a "previous" copy) first in the GRUB menu started an old kernel whose modules the upgrade had removed, so `/boot` and the firewall failed to load after a reboot.

## 0.1.12 — 2026-11-16

- **uQ Settings' Bindle page is rebuilt** (needs uQ 0.3.0 or later; `sudo bindle first-login install` writes the new files): five tabs. **Setup** (the profile and what is missing, Pick apps again), **Updates** (bindleUP: what each stage would do, Update now with progress and a preview first, or one stage alone), **What's new** (apply what Bindle ticks, with a preview), **History** (what Bindle changed, Undo with a preview) and **Preferences** (every bindleUP setting, the daily check, per-app update sources).
- **New command line for it**: `bindle config show|set|reset` (bindleUP's preferences and the daily check as typed keys, instead of editing `upgrade.toml`), `bindleup --ndjson` (an unattended run as one JSON event per line; the AUR is left to a terminal) and `bindleup --check --ndjson`, `bindleup --dry-run --json` (what an update would change), `bindle undo --list --json` and `undo --dry-run --json`, `bindle whats-new --dry-run --json`. `bindleup --json` now also works with `--dry-run`.

## 0.1.11 — 2026-11-16

- **`bindleupgrade` is now bindleUP, and the command is `bindleup`** (`bindle upgrade` still works). Nothing else about it changes. The old `/usr/local/bin/bindleupgrade` link is no longer installed or recognised; a daily-check timer made by `bindo update-notify on` points at the old path, so run `bindo update-notify on` again to rewrite it. The window is titled "bindleUP · System update".
- **appOFholding 0.8.1** (was 0.5.0): updating apps from local files, dropped AppImages moved into the library by default, and readable radio buttons and check boxes in the Gruvbox themes.
- **kyko is no longer offered** until it has a stable release: Bindle now pins only stable, CI-green tagged releases, and kyko has had previews only. What's new lists it as a clean-up; a kyko you already have stays installed.
- **The ArtCraft suite is no longer offered** (PhotoCraft, LightCraft, FilmCraft, VectorCraft, EffectCraft, DesignCraft, PdfCraft, WordCraft, DeckCraft, GridCraft): they are still in alpha, and Bindle now offers only stable apps. What's new lists them as a clean-up; copies you have stay installed. They come back once they leave alpha.
- **`make check-pins`** (and CI): fails a pinned release that is a draft, a pre-release or not CI-green, or whose asset changed, and lists newer stable releases, or for a failing pin the last good one; `scripts/check-pins.py --bump` moves the pins there.

## 0.1.10 — 2026-11-16

- **Opt-in choices** (offered, never ticked by default): don't freeze when memory runs out (MGLRU thrashing protection); `noatime` on Btrfs; night light on window managers (gammastep at the timezone's location, no location service); Firefox without telemetry or studies (a system policy); hardware video decoding in Firefox on NVIDIA desktops (libva-nvidia-driver; it turns off Firefox's media sandbox); GnuPG's passphrase in a window; a gentler lockout after wrong passwords (5 tries, 2 minutes); Wi-Fi address privacy (random while scanning, stable per network); firejail; auditd; real-time audio for music apps.
- New bindo recipes: `sleep-check` (sleep mode, wake devices, recent sleeps, AMD's s2idle checker) and `usbguard` (allow only the USB devices plugged in now); `health` says when an AMD CPU runs without CPPC.
- **Hardware Bindle now recognises** (it reads the maker and model, USB and PCI devices, monitors and the battery; each module applies only where found):
  - Thunderbolt docks approved (bolt, and Plasma's settings page); fan curves on desktops (CoolerControl, opt-in); fingerprint for sudo, run0 and admin prompts (opt-in); brightness for external monitors (ddcutil); the webcam of 2021+ Intel laptops (libcamera); security keys and smartcard readers; a bigger console font on high-resolution screens; more game controllers (game-devices-udev) and Xbox controllers over Bluetooth (xpadneo); temperature sensors on desktops.
  - ASUS ROG and TUF laptops (asusctl), 8th-generation ThinkPads' slow-down fix (throttled), Framework's light sensor and tools, Dell's settings tools; handheld gaming PCs (InputPlumber: ROG Ally, Legion Go, MSI Claw, Steam Deck); Huion and XP-Pen tablets (OpenTabletDriver); HP and Epson printer and scanner drivers.
  - An opt-in charge limit at 80% where the battery supports one (not on Plasma or GNOME, which have their own).
  - Hybrid NVIDIA laptops power the GPU off when idle (Turing) or get Dynamic Boost (Ampere and newer). The fingerprint module now needs a fingerprint reader, not just a laptop. VM disk images on Btrfs skip copy-on-write.
- **Everyday defaults from the ArchWiki, CachyOS and other distros** (23 modules, most on by default, each one package or drop-in file; What's new offers them):
  - System: logs and crash dumps kept under 1 GB; drivers keep loading after a kernel update until you restart (kernel-modules-hook); a list of new .pacnew files after each update; 15-second stop timeouts instead of 90; writeback in small batches with 8 GB or more of memory (no frozen desktop copying to USB); NMI watchdog off; no audio clicks on desktops; Wi-Fi rules for your country (from the timezone); light kernel hardening that leaves debuggers, Flatpak and games alone.
  - Desktop: the GNOME keyring unlocked at login on greetd and ly (uQ, window managers); KDE Wallet on Plasma, and on Hyprland and other greetd/ly logins (uQ's login screen needs its own change); folders open in Dolphin or Files on window managers, not in whichever app claimed folders last (kitty, an editor); Dolphin searches by file name only outside Plasma (Baloo stops reading every file); Dolphin gets Plasma's extras on uQ and Hyprland: Open as administrator, the file type editor, AVIF/HEIF/JPEG XL and AppImage thumbnails, iPhones; video thumbnails in Dolphin (the thumbnailer packages, and dropping a stale preview-plugin list from dolphinrc that kept Dolphin from using them); ssh-agent for the session; hardware video decoding in Chromium, Brave, Chrome and Vivaldi on Intel and AMD; Flatpak apps follow your theme; qt6ct and one cursor on window managers, Qt following GTK on GNOME; CJK text in your language's shapes; bash suggests packages for unknown commands; screen locking on Sway and niri; a sign-in prompt for captive Wi-Fi on window managers.
  - Gaming: ntsync for Wine and Proton; 10 GB shader caches.
  - Power profiles on desktops too, not only laptops. Intel graphics get the right video driver for their generation (i965 for Haswell and older, Quick Sync on newer). NVIDIA's 580 driver for older cards gets its suspend services (the 595+ open driver doesn't need them).
- **More bindo recipes (24 → 64 shipped), some after Universal Blue's ujust and similar tools.** Fixes (new): `restart-audio`, `bt-headset-fix`, `restart-network`, `downgrade`. Security (new): `tpm-unlock`, `secure-boot`, `encrypted-dns`, `save-panics`, each refusing a setup it could leave unable to start. System: `firmware-update`, `bios`, `changed-configs`. Setup: `virtualization`, `distrobox`, `tailscale`, `input-group`. Toggles: `sysrq`. Diagnose (new): `report` (one file to share when asking for help), `disk-usage`, `health` (drive SMART, temperatures, battery wear), `network` (says what doesn't work), `boot-time`, `gpu`. Maintenance: `trim`, `scrub`, `clean-cache`, `clean-coredumps`, `rebuild-boot`, `dkms-rebuild`, `unlock-pacman`, `aur-rebuild`. Apps (new): `flatpak-repair`, `default-browser`, `default-apps`. Toggles: `autologin`, `hibernate` (only where systemd resumes by itself: UEFI and a systemd-based initramfs), `wake-on-lan`, `printer-sharing`.
- **Recovery: Roll back, Rebase and Factory Restore.** A Recovery window (`bindle recovery`, `bindo recovery`) and terminal commands, each explaining what stays and what goes before anything runs. See docs/RECOVERY.md.
  - `bindle rollback`: pick a snapper snapshot; it becomes the system from the next start, the current one kept aside (`bindo rollback-cleanup` deletes it later). The kernels on /boot follow the snapshot, so it starts with matching modules. Btrfs with snapper.
  - `bindle rebase`: the system as installed again, Bindle's setup included: reinstalls those packages, puts back removed ones, resets their changed config files (your copies kept; `--undo-config` puts them back), applies Bindle's setup again. Your home, your own packages and the machine's own settings stay.
  - `bindle restore`: Factory Restore. Downloads and checks the Arch ISO (or uses the one kept on the computer when offline), then restarts into Bindle's installer from it with this computer's disk picked; nothing is erased until the installer's last question. `--keep-image on` keeps a checked copy, which also adds "Roll back the system" to the boot menu for when the system won't start.

- **Bazaar is the Flathub app store.** With Flatpak, Bindle now installs Bazaar on every desktop and makes it open Flathub's Install buttons and `.flatpakref` files. Flatseal (what each Flatpak app may reach) and Warehouse (manage installed Flatpaks: batch remove, clear data, roll back) come with it. Discover and GNOME Software can be picked on any desktop too (before: only on Plasma and GNOME), no longer pre-selected.
- **Screenshots of an area on Print.** On Hyprland, Sway, niri, labwc, i3, bspwm, awesome and qtile, Print now lets you select part of the screen and opens it in satty to mark up: Enter saves it to Pictures/Screenshots, the copy button puts it on the clipboard too (`bindle-screenshot`: grim and slurp on Wayland, maim on X11). Bindle adds the binding to the window manager's own config between `BEGIN Bindle` and `END Bindle` lines, starting from the window manager's default config when there's none; on Hyprland it waits for the config Hyprland writes at its first start. `bindo screenshot-key-remove` takes it out again. Flameshot is no longer installed with i3, bspwm, awesome and qtile.
- **Clipboard history is CopyQ, with a tray icon.** Wherever Bindle sets up clipboard history (every desktop but Plasma, GNOME, COSMIC and the ones with their own), CopyQ now keeps it, Wayland included: click the tray icon to paste something from earlier. Items go 30 days after they were copied; the tray menu switches that to 60 or 90 days or forever (also `bindo clipboard-keep`). It replaces the cliphist history; What's new offers to remove cliphist's service.
- Not shown any more: Welcome's "archinstall enabled zram with default sizing" advice once Bindle's zram module has tuned swap.
- Setting the system up from the live ISO no longer stops at snapper: `snapper create-config` now runs without D-Bus inside the chroot, where the bus belongs to the live system.
- Self-update trusts only the new release key (`CCFF 34EE … DE41 DF9C`). `install.sh` still accepts the retired key `75C4 0FA1 … E02D 69FC`, but only for releases up to v0.1.9, so `BINDLE_VERSION=v0.1.8` still installs.

## 0.1.9 — 2026-11-15

- **A second release signing key.** Bindle and `install.sh` now also trust `CCFF 34EE 4CC0 AE7C C686 5E92 7C23 0DE2 DE41 DF9C`, alongside `75C4 0FA1 … E02D 69FC`. This release is signed by the first key; releases after it are signed by the new one. Update to this release before the next one, or self-update will refuse it.
- **What's new after upgrading Bindle.** Every apply now records, in its snapshot, the Bindle version and what each module puts on the system. A newer Bindle compares that record with your saved selection planned by its catalog: new modules for this computer, modules it now sets up differently (with the exact changes, file diffs included), modules that left Bindle (with an optional clean-up), and optional modules new in this version. Pick what to apply; it goes through the usual review and snapshot, so `bindle undo` reverts it. `bindle whats-new` (also `--list`, `--json`, `--yes`), a terminal page, a page in the window (`bindle-gui --whats-new`), `bindo whats-new`; it opens by itself the first time a newer Bindle runs, and after bindleupgrade updates Bindle. Installs from before records are compared with what is installed. The record also lives in a ledger that outlives snapshots and follows `bindle undo` (`/var/lib/bindle/record.json`, and the user's for applies that needed no root). Single packages, files and services can be left out of an item; modules you took out of your setup are their own group; "Not now" asks again next time, "Skip this version" waits for a newer Bindle, and "Hide" drops an item until it changes again. See docs/UPGRADE-PATH.md.
- New `bindo` (also `bindle bindo`): Bindle's recipe runner, like Universal Blue's `ujust`. `bindo` lists named system tasks by category; `bindo <recipe> --help` shows one, its arguments and its script; `bindo <recipe> [args]` runs it. 24 shipped recipes: system info, kernel, errors, failed services, logs; cache, journal and thumbnail cleaning, orphans, `.pacnew` merging, mirrors, keyring repair, `update`, `update-notify`, `upgrade-prefs`; re-running parts of Bindle's setup (`setup`, `add`, `printer`, `undo`, `whats-new`), `completions`; Bluetooth, SSH and Wi-Fi toggles. Recipes are TOML (no `just` needed), can be added or overridden in `/etc/bindle/recipes` and `~/.config/bindo/recipes`, and `[admin]` ones ask for the password through polkit (pkexec, else run0). Tab completion: `bindo completions bash|zsh|fish`. See docs/BINDO.md.
- New `bindleupgrade` (also `bindle upgrade`): the whole system updated in one guided run. Reads the Arch news since your last upgrade first (and holds the upgrade when a post asks for a manual step), takes a snapper snapshot, refreshes stale mirrors, shows and installs package updates, then the AUR (paru), Bindle's pinned release apps, Flatpak and (with `--firmware`) firmware; afterwards lists `.pacnew` files and orphans and says when a reboot is due. Every stage can be skipped (`--skip`, `--only`), a failed stage doesn't stop the others, and a table sums up each stage at the end. `--check` only reports. See docs/UPGRADE.md.
- `bindle first-login install` links `/usr/local/bin/bindo` and `/usr/local/bin/bindleupgrade` to `bindle`.
- Every approval goes through polkit (`pkexec`): `bindle apply`, `undo` and `bindleupgrade` no longer ask through sudo in a terminal; on a console pkexec asks as text. Only the live-ISO install inside arch-chroot still uses its passwordless sudo grant, where nobody is asked.
- bindleupgrade runs `pacman -Syu` on your terminal, so pacman asks its own questions (replacements, conflicts) instead of failing on them; `--yes` keeps the unattended `--noconfirm`.
- bindleupgrade keeps each pinned app on the source it came from: only copies Bindle installed from its releases are updated, ones from the Arch repos or the AUR are left alone. Preferences in `~/.config/bindle/upgrade.toml` (`bindo upgrade-prefs`) switch that per app or for all, and set the mirror-refresh age and the news window.
- `bindleupgrade --gui`: the update in Bindle's desktop window (same look and parts as the wizard). Check lists what each stage would do with a switch per stage (leave it out, upgrade, remove orphans, "I've read the news", install firmware); Update shows each stage beside the output and ends on a result table with Restart, "Update the AUR" in a terminal and the log.
- bindleupgrade checks more before and after upgrading: Bindle's own new release (verified against its signed SHA256SUMS, installed, then the run goes on with it); free space on `/`, `/boot` and the EFI partition and battery power before package upgrades (thresholds in the preferences); Hyprland updates beyond uQ's pin, which pacman would refuse; AUR packages a library update broke (rebuild-detector, now installed with paru), rebuilt; uQ's setup again after uQ updates, and its login screen kept on; services that newly failed; new Bindle features for this machine and files that changed since Bindle wrote them; DKMS modules and boot images for the new kernel before offering a restart ("don't restart yet" with the fix). Flatpak removes unused runtimes after updating; fwupd's list is refreshed when old.
- bindleupgrade: "new features" counts only modules that would change something (before, every module looked new, since planning one alone also brought the profile's defaults); after a kernel update it offers to build missing DKMS modules and boot images instead of only naming the commands, and reads /boot through the root helper when it is root-only; a `--check` no longer refreshes fwupd's list; after a uQ update it no longer runs `uqctl reload` (the old uqd would render the new templates) and says to log in again.
- `bindo update-notify on`: a daily user timer runs `bindleupgrade --check --notify`; its notification's Update button opens the update window.
- The update window shows the stages the news, battery or a switched-off upgrade hold back as held (they no longer count as work), and ends with "don't restart yet", "Open Bindle" for new features, and "Go on" after Bindle updated itself.
- bindleupgrade asks for the password once: if it is dismissed, later stages fail at once instead of asking again.
- Catalog modules can carry bindo recipes (`[[module.recipe]]`), listed once the module is installed: `snapshot`/`snapshots` (snapper), `firewall` (ufw), `power-profile`, `printers`/`printing-restart`, `flatpak-clean`.
- The desktop window is redesigned around showing what happens. Welcome puts "Already working" beside "Bindle suggests". Profile picks a base (Minimal, Desktop) and any add-ons on top (Gaming, Developer; `addon = true` in `profiles.toml`). Choose shows a category's modules in groups, as a grid, list or compact view, with search, filters (selected, recommended, hide what isn't available, only safe changes), Select all / Select none per group, and details opening inline under the group. A new Summary page is an editable receipt of every pick. Review starts with what the run costs (password, restart, undo), then Will install, Will change and Left alone. Apply shows the running step large, then Restart now, Undo these changes, Open the log and a few things to try first.
- Other drives can be mounted at startup: Bindle detects filesystems the system doesn't use itself and offers one module per drive (Hardware → "Drives to mount at startup", nothing pre-selected). Each is mounted by UUID with `nofail` under `/mnt/<label>`, in one marked block in `/etc/fstab` that a re-run replaces rather than duplicates; `bindle undo` restores the file. See docs/DRIVES.md.
- New modules for the desktop, profiles: `everyday-apps` (file manager, archive manager, image and PDF viewers, calculator and a disk tool matching the desktop, on desktops that came without them), `spellcheck` (dictionaries for the system language), `disk-formats` (exFAT, FAT and NTFS tools), `appimage` (FUSE 2, so AppImages run), `bash-completion`, `vpn-plugins` (OpenVPN in NetworkManager), `clipboard-history` (on bare Wayland desktops) and `copyq` (on X11 window managers). Offered, not pre-selected: `marknote` (Markdown notes; pre-selected with uQ) and `xournalpp` (handwritten notes, PDF annotation).

- NVIDIA's own driver is now the recommended one wherever it can be installed, with the open-source driver as the other choice. New module `nvidia-legacy` (NVIDIA driver for older cards): GTX 750/900/10-series and Titan V cards (Maxwell, Pascal, Volta) get `nvidia-580xx-dkms` and `nvidia-580xx-utils` from the AUR, with the kernel's headers and, with multilib, the 32-bit libraries. It is pre-selected on those cards; before, they got nouveau and a note. Untick it (or pick the open-source driver on `bindle install`'s Hardware page) to stay on nouveau. Kepler and older cards still get nouveau.
- With NVIDIA's own driver (nvidia-open-dkms, nvidia-dkms, nvidia-580xx-dkms, nvidia-470xx-dkms, installed by Bindle or already there), the initramfs no longer carries nouveau and the 100+ MiB of NVIDIA firmware it declares: Bindle writes `/etc/mkinitcpio.conf.d/zz-bindle-nvidia.conf`, with HOOKS copied from mkinitcpio.conf minus `kms` (Arch wiki: NVIDIA, Early loading) and, on hybrid laptops, the integrated GPU's driver (`i915`, `xe` or `amdgpu`) in MODULES to keep its early KMS, then runs `mkinitcpio -P`. mkinitcpio.conf itself is not edited. Back on nouveau, the drop-in is removed and the images rebuilt. Check with `lsinitcpio -a /boot/initramfs-linux.img | grep nouveau` (for a UKI, extract it first: `objcopy -O binary --only-section=.initrd /boot/EFI/Linux/arch-linux.efi initrd.img`): nothing found.
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
- Inside a chroot (setting up the new system from the live ISO), the root helper starts through the install's temporary passwordless sudo grant instead of pkexec, and enables services without starting them.
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
