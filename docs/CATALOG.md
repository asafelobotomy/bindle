# Catalog format

One TOML file per category under `catalog/<distro>/`, registered in
`crates/bindle-core/src/catalog.rs` (`ARCH_FILES`). Profiles live in
`catalog/profiles.toml`. The catalog is validated at load time; `cargo test` fails
on a broken reference.

A profile with `addon = true` (Gaming, Developer) is offered by the desktop
wizard as a tick on top of a base profile (Minimal, Desktop) instead of on
its own. Its own modules are the ones that list it in `profiles` and list no
base profile; ticking it adds just those, and the saved selection is the base
profile plus `include`s. The terminal wizard still lists every profile.

```toml
[category]
id = "performance"
name = "Performance"
description = "Memory, I/O and build tuning."
order = 30                      # wizard ordering

[[module]]
id = "io-schedulers"            # unique across the catalog
name = "I/O schedulers"
group = "Disks"                 # heading within the category; groups show in first-appearance order
description = "…"
risk = "safe"                   # safe | moderate | advanced
default = true                  # pre-selected in every profile
profiles = ["gaming"]           # pre-selected in these profiles
when = ["ssd", "!virt"]         # all must hold; see tags below
requires = ["multilib"]         # pulled in automatically
conflicts = ["tlp"]             # cannot be selected together
packages = ["pkg"]              # repo packages (one pacman transaction)
package_conflicts = ["pkg-old"]  # installed packages `packages` replace: removed after confirmation
                                # (pacman -Rdd), or the module is skipped with --yes
replace_after_install = false   # true: remove package_conflicts after this module's AUR/release
                                # install and root commands (e.g. an import that reads them first)
aur = ["pkg-bin"]               # installed as the user via the AUR helper
enable = ["foo.timer"]          # systemctl enable
groups = ["docker"]             # usermod -aG for the invoking user
pacman = { Color = "", ParallelDownloads = "10" }   # [options]; "" = flag
repos = ["multilib"]            # uncomment a repo section
root_commands = [
  ["ufw", "default", "deny", "incoming"],                # bare argv
  { argv = ["ufw", "allow", "1714:1764/tcp"],            # or a guarded command:
    when = ["kdeconnect"],                               #   only with these tags
    unless = [{ path = "/etc/ufw/user.rules", contains = "--dport 1714:1764" }] },
]
user_commands = [{ argv = ["xdg-user-dirs-update"], unless = [{ path = "~/.config/user-dirs.dirs" }] }]
# A user command may map exit codes to a manual step: the apply goes on and the
# message is listed at the end. Any other non-zero exit fails the step.
#   { argv = ["uqctl", "setup"], manual = { 3 = "add the lines it printed to hyprland.lua" } }
builtin = "zram"                # computed steps (gpu, nvidia-legacy, microcode, zram, aur-helper, desktop, everyday, spellcheck)
notes = ["Shown on the review screen."]

[[module.release]]              # a pinned GitHub release asset (repeatable)
repo = "owner/repo"
tag = "v1.2.3"
asset = "app-1.2.3-1-x86_64.pkg.tar.zst"   # .pkg.tar.zst → pacman -U; .AppImage → ~/.local/bin
sha256 = "…"                    # of the asset; a download that differs is refused
name = "app"                    # package name, or the AppImage's command name

[[module.files]]
path = "/etc/udev/rules.d/60-bindle-ioschedulers.rules"
mode = 0o644
content = '''
# Managed by Bindle
…
'''
```

## Modules that install from the AUR or a release

A module with `aur` or `[[module.release]]` has its `enable` units and
`root_commands` run after the AUR and release installs, so they can use what
was installed (scdr's `scdrd.service`, `scdrctl import`). Its
`package_conflicts` are still removed before installing, unless it sets
`replace_after_install`: then they go after its root commands and before its
units (scdr imports ananicy's rules, then removes ananicy, then starts).

## Release assets

For apps that ship on GitHub rather than in the repos or the AUR. Bindle
downloads the asset as you over HTTPS, refuses it unless its sha256 matches the
pin, then installs a `.pkg.tar.zst` through the root helper (`pacman -U`, which
pulls its dependencies from the repos) or puts an `.AppImage` in `~/.local/bin`
with a desktop entry (`~/.local/share/applications/bindle-<name>.desktop`).
AppImage modules should list `fuse2` in `packages`. A package counts as
installed by name; an AppImage when its desktop entry names the pinned asset,
so bumping `tag`/`asset`/`sha256` upgrades it on the next apply. Take the
sha256 from the release's `SHA256SUMS` when it has one. `make check-packages`
with `--aur` checks every pinned URL still resolves.

## Command guards

User commands run after everything else, unless they set `before_aur = true`: then they run
right after the packages install, before the AUR helper and AUR builds (rustup's
`rustup default stable`, which every Rust PKGBUILD needs).

Commands are re-planned on every run unless they say how to tell they're done.
`unless` is a list of file checks, read the same way the planner reads files
(as the invoking user, or inside `--sandbox`); when **all** hold, the command is
dropped from the plan. A check holds when the file is readable and, if given,
contains `contains` and/or equals `content` exactly. A unit masked with
`systemctl mask` is a symlink to `/dev/null`, so it reads as `content = ""`.
Root-only files (mode 0600/0640) never hold; pick a world-readable witness
(e.g. `/etc/conf.d/snapper` rather than `/etc/snapper/configs/root`).
User-command paths may start with `~/` (the user's home from the passwd database).
`when` on a command takes the same tags as a module's `when`.

`cargo test` validates the catalog (`crates/bindle-core/src/catalog/validate.rs`): unique ids,
references, symmetric and non-default `conflicts`, valid names (no leading `-`), file paths
(absolute, no `..`, one module per path, mode ≤ 0o777), `aur` requiring `aur-helper`, `when`
tags (below; `desktop:<id>` must name a desktop in `desktops.toml`), absolute
file and check paths.

## `when` tags

`distro:<id>`, `cpu:<intel|amd|other>`, `gpu:<nvidia|amd|intel|virtual>`,
`fs:<root fs>`, `boot:<systemd-boot|grub|limine|refind|unknown>`,
`session:<wayland|x11|tty>`, plus flags `ssd`, `laptop`, `bluetooth`, `virt`,
`multilib`, `zram-active`, `zswap`, `hibernate`, `aur-helper`, `desktop`, `nm`,
`kdeconnect` (installed, or in a detected desktop's essentials),
`snapshots-mount` (`/.snapshots` is its own mount, e.g. archinstall's
`@.snapshots`), `hybrid-gpu` (NVIDIA plus Intel/AMD), `nvidia-legacy` (a Maxwell,
Pascal or Volta NVIDIA card: NVIDIA's own driver for it is the 580 branch),
plus `desktop:<id>`, `toolkit:<qt|gtk|other>`, `display:<wayland|xorg>` and
`installer:<archinstall|pacstrap>`, and `installed:<package>` (the package is
installed now). Prefix with `!` to negate.

Modules the installer already set up (see [ARCHINSTALL.md](ARCHINSTALL.md)) are
reported as provided and never planned.

## Generated modules

Drives to mount at startup are not catalog data: `Catalog::with_drives(&facts)` adds one
module per detected drive (`mount-<uuid>`, Hardware → "Drives to mount at startup", builtin
`mount:<uuid>`, `when = ["mountable:<uuid>"]`). They are never in a profile. See
[DRIVES.md](DRIVES.md).

## Desktops

`catalog/desktops.toml` describes each archinstall desktop: how to recognise it
(`detect`, `archinstall` package set), its greeter, and the `essentials`/`nm`/`bt`
packages the `desktop` builtin adds. Check package names with `make check-packages`
(it also covers greeters, archinstall's lists, and the `PACKAGES` consts in
`crates/bindle-core/src/builtins/`, which unit tests keep in sync with builtin output).
Run `bindle detect` to see the facts for the current machine.

A desktop archinstall has no profile for sets `base` (the archinstall desktop
`bindle iso` installs), `includes = [base]`, and `module` (the catalog module that
finishes it on first login; `bindle iso` adds it to the saved selection). While
that module is planned, the `desktop` builtin rounds out this desktop, not its base.
A desktop's `apps` are modules `bindle iso` pre-selects with its `module` on
first login; each stays an ordinary module the user can untick. uQ's are
`scdr`, `crumbs` and `noti-fi`.
uQ is the one such desktop: Hyprland plus the `uq` module (AUR `uq` and
`uq-hyprland-pin`, then `uqctl setup --apps` as the user, skipped once
`~/.config/hypr/hyprland.lua` loads uQ and `~/.config/qt6ct/qt6ct.conf` loads its colours;
`--apps` themes GTK, Qt and the terminals, so uQ's own Settings isn't left unstyled). `uqctl setup` exit codes: 0 done, 1 error
and 2 (ran as root) fail the step, 3 (the user's own `hyprland.lua` was kept) is
listed as a manual step. The module also installs `greetd` and runs
`uqctl greeter enable` as root (skipped once `display-manager.service` is
greetd), so uQ's login screen replaces the display manager from the next boot.
uQ's `greeter` in `desktops.toml` stays `sddm`: archinstall can only install its
own greeters, so that one starts the first login, then uQ's takes over.

In a uQ session the first-login wizard holds back uQ's Welcome
(`$XDG_RUNTIME_DIR/uq/welcome.defer`) and opens it with `uqctl welcome` when it
ends (`crates/bindle-cli/src/uq_handoff.rs`). `bindle first-login install` also
installs uQ Settings' "Re-run setup" page from `uq/pages/bindle/`.

### Recommended desktop

`status = "recommended"` marks the one desktop `bindle iso` pre-selects and labels
"(recommended)"; `status = "preview"` labels it "(preview)" and pre-selects nothing.
uQ is Bindle's default desktop. It was gated as `status = "preview"` until `uq`
was on the AUR and a fresh-VM `bindle iso` → uQ install passed (docs/VM-TEST.md);
both held on 2026-11-10, so it is `"recommended"`. The steps used to flip it:

1. `python3 scripts/check-packages.py --aur` prints
   `preview desktop uq: uq uq-hyprland-pin on the AUR; it can be recommended`.
2. In `catalog/desktops.toml`, change uQ's `status = "preview"` to `"recommended"`.
3. Update the assertions in `crates/bindle-core/src/desktops.rs`
   (`uq_rides_on_hyprland_and_is_a_preview_until_published`) and
   `crates/bindle-cli/src/tui/iso.rs` (`opens_on_the_requested_then_the_recommended_desktop`),
   then `make test`.

Unattended runs (`bindle iso --yes` without `--desktop`) never pick a desktop on
their own, recommended or not.

## Arch modules

| Category | Modules |
|---|---|
| System | pacman-qol, mirrors, multilib, build-tools, aur-helper, microcode, firmware-updates, fstrim, man-pages |
| Hardware | gpu-driver, nvidia-legacy, audio-firmware, audio-pipewire, bluetooth, power-profiles, tlp, tlp-rdw, thermald, fingerprint, printing, scanning-qt, scanning-gtk, solaar, piper, openrgb, android-tools |
| Performance | zram, xzram, oomd, earlyoom, io-schedulers, scdr, scdr-sched-ext, scdr-devbox, scx-schedulers, kernel-zen, makepkg-parallel, makepkg-native |
| Desktop & apps | desktop-essentials, uq, noti-fi, crumbs, uq-dev, fonts, nerd-fonts, codecs, archives, user-dirs, everyday-apps, spellcheck, disk-formats, appimage, bash-completion, vpn-plugins, flatpak, flatpak-discover, flatpak-gnome-software, media-players, spotify, libreoffice, obsidian, marknote, xournalpp, text-editor, yakuake, kyko, emobie |
| Internet | firefox, chromium, brave, brave-origin, vivaldi, zen-browser, google-chrome, thunderbird, signal, telegram, element, proton-mail, proton-mail-bridge, proton-pass, proton-pass-cli, proton-vpn, proton-authenticator, proton-drive-cli, syncthing, nextcloud-client, tailscale, qbittorrent, qbittorrent-enhanced, qbit-plugin-dl, hauler, av-morning-star |
| Creative | gimp, krita, inkscape, darktable, rawtherapee, digikam, kdenlive, handbrake, obs-studio, audacity, blender, photocraft, lightcraft, filmcraft, vectorcraft, effectcraft, designcraft, pdfcraft, wordcraft, deckcraft, gridcraft |
| Development | rustup, python, nodejs, js-runtimes, go, c-cpp, java, dotnet, zig, mise, git-tools, jujutsu, cli-tools, neovim, helix, code-oss, vscodium, vscode, zed, ghostty, kitty, alacritty, fish, zsh, docker, podman, distrobox, virt-manager |
| AI tools | claude-code, codex, gemini-cli, grok-build, copilot-cli, opencode, claude-desktop, cursor, ollama, ollama-cuda, ollama-rocm, lmstudio |
| Gaming | steam, gamemode, mangohud, goverlay, gamescope, wine, protontricks, protonup-qt, lutris, heroic, bottles, discord, vesktop, lact, emulators, pcsx2, azahar, moonlight, prismlauncher, romforge, reshadelinux |
| Security | ufw, firewalld, opensnitch, snapper, grub-btrfs, timeshift, btrfs-maintenance, secure-boot-tools, oysterav, keepassxc, bitwarden, backup-gtk, backup-deja-dup |
