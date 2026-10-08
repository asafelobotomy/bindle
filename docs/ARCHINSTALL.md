# Building on archinstall

Bindle assumes a **desktop install made with archinstall** and starts from
what archinstall already did, so it never redoes or contradicts those choices.
Findings below are from archinstall **v4.5** source (upstream `main`, 2026-10-03).

## What archinstall leaves on the installed system

| Artifact | On the new system? | Contents |
|---|---|---|
| `/var/log/archinstall/install.log` | **Yes**: `Installer.sync_log_to_install_medium()` copies it into the target on exit | Every log line, including a DEBUG entry ` -- Chosen configuration --` followed by the full config JSON (`ArchConfig.write_debug()`, called by the guided script) |
| `user_configuration.json` | **No** by default: `config.save()` writes it to `/var/log/archinstall` on the **live ISO** | Same JSON. Only reaches the target if the user picks "Save configuration" and a path under `/mnt` |
| `user_credentials.json` | No | Never read by Bindle |
| `/var/log/pacman.log` | **Yes**, since pacstrap writes into the target | Every installer transaction: `pacman -b /mnt//var/lib/pacman/ -r /mnt … <packages>` |

The logged config is credential-free: `safe_config()` drops user passwords,
the root password and the disk-encryption password; auth is reduced to U2F flags.

## How Bindle recovers the install (best source first)

1. **install.log** → exact config: profile and desktops (+ Plasma flavor), greeter, GPU
   driver, kernels, bootloader + UKI, root filesystem, LUKS, Snapper/Timeshift, zram +
   algorithm, network backend, audio, Bluetooth, power, printing, firewall, fonts,
   multilib, pacman options, locale, timezone, extra packages, services.
   Also reads a saved `user_configuration.json` there.
2. **pacman.log** → rebuilt. archinstall installs each feature in its own transaction,
   so each one maps back to a menu choice (zram-generator ⇒ zram, `tuned tuned-ppd` ⇒
   power, `gnome gnome-tweaks` ⇒ GNOME, `gdm` ⇒ greeter, …). Unrecognised transactions
   are the user's *additional packages*. GPU drivers are identified by vendor marker
   packages, so older archinstall package sets still match.
3. **Live system** fills what's left: bootloader (`LoaderInfo` EFI var), display manager
   (`display-manager.service`), desktops (signature packages), multilib, enabled units.

`bindle archinstall [--json]` prints the result; the wizard opens on it.

> Tip for users: in archinstall's menu, *Save configuration* → `/mnt/var/log/archinstall`
> keeps the exact config on the new system even if the log is lost.

## archinstall choice → Bindle module

| archinstall | Bindle treats as done | Effect |
|---|---|---|
| Audio: PipeWire | `audio-pipewire` | ✓ locked in wizard |
| Bluetooth | `bluetooth` | ✓ |
| Power: power-profiles-daemon / tuned (+tuned-ppd) | `power-profiles` | ✓; `tlp` skipped as a conflict |
| Print service | `printing` | ✓ |
| Firewall: ufw / firewalld | `ufw` / `firewalld` | ✓; the other skipped as a conflict |
| Fonts (all five) | `fonts` | ✓ |
| Optional repo: multilib | `multilib` | ✓ |
| Btrfs snapshots: Snapper | `snapper` | ✓ |
| Swap on zram | `zram` stays selectable | archinstall sets no size and no sysctls; Bindle adds the tuning (unless XZram already does) |
| GPU: *All open-source* | advisory | lists the drivers for vendors you don't have |
| Network: plain iwd + desktop | advisory | desktop network panels expect NetworkManager |
| Always: `fstrim.timer` | not re-enabled if still enabled | |

## Desktops (21) and what Bindle adds

Source of truth: [`catalog/desktops.toml`](../catalog/desktops.toml) (the
`desktop-essentials` module). `nm` / `bt` extras only apply when NetworkManager /
Bluetooth hardware is present.

| Desktop | Display | Toolkit | archinstall greeter | Bindle adds |
|---|---|---|---|---|
| KDE Plasma | Wayland | Qt | plasma-login-manager | core KDE apps (plasma-meta has none): konsole, dolphin, kate, ark, spectacle, gwenview, okular, kcalc, kdeconnect, filelight, thumbnailers |
| GNOME | Wayland | GTK | gdm | extension-manager, gnome-shell-extensions, ffmpegthumbnailer, gvfs-mtp |
| Cosmic | Wayland | — | cosmic-greeter | gvfs-mtp |
| Xfce4 | Xorg | GTK | lightdm-gtk | xdg-user-dirs-gtk, gvfs-mtp, nm-applet, blueman |
| Cinnamon | Xorg | GTK | lightdm-gtk | gvfs-mtp, pavucontrol, blueman |
| Mate | Xorg | GTK | lightdm-gtk | gvfs-mtp, nm-applet, blueman |
| Budgie | Wayland | GTK | lightdm-slick | gvfs, pavucontrol, nm-applet, blueman |
| Lxqt | Xorg | Qt | sddm | pavucontrol-qt, gvfs, nm-applet, blueman |
| Deepin | Xorg | Qt | lightdm-gtk | deepin-extra |
| Enlightenment | Xorg | — | lightdm-gtk | pavucontrol |
| Hyprland | Wayland | — | sddm | waybar, hyprpaper, hyprlock, hypridle, hyprpolkitagent, portals, cliphist, nwg-look |
| Sway | Wayland | — | lightdm-gtk | mako, wl-clipboard, portals, polkit agent |
| niri | Wayland | — | lightdm-gtk | xwayland-satellite, portal, polkit agent, wl-clipboard |
| niri + DankMaterialShell | Wayland | — | dms-greeter | xwayland-satellite, cliphist |
| Labwc / River | Wayland | — | lightdm-gtk | bar, launcher, notifications, lock, portals, polkit agent |
| i3 / Awesome / Bspwm / Qtile / Xmonad | Xorg | — | lightdm-gtk (Awesome: none) | compositor (picom), launcher (rofi), notifications, bar where missing, polkit agent |

## Opening Bindle on first login

`bindle first-login install` (as root) installs:

| File | Purpose |
|---|---|
| `/usr/local/bin/bindle` | the binary (a copy of whatever ran the command) |
| `/etc/xdg/autostart/bindle-first-login.desktop` | desktops: runs `bindle first-login run` at login; Bindle opens the desktop's own terminal |
| `/etc/systemd/user/bindle-first-login.service` (enabled globally) | window managers without XDG autostart: waits for this user's graphical session (logind + Wayland/X11 socket), then opens a terminal; a short-lived lock in `$XDG_RUNTIME_DIR` prevents a second window where autostart also fires |
| `/etc/profile.d/bindle-first-login.sh` | console logins (TTY, `startx` users); skipped over SSH or inside a graphical session (`--no-tty-hook` to omit) |
| `/usr/local/share/applications/bindle.desktop` + icon | a normal launcher for later |

**First login** opens the wizard. **Later logins**, if setup isn't finished,
show a desktop notification instead ("Finish setting up your system" with
*Set up now* / *Don't remind me*), kept in the notification tray until handled.
It goes straight to the freedesktop notification service over D-Bus, so
libnotify isn't needed. With no notification server, the wizard opens as
before. Console logins get a one-line reminder. Finishing the wizard, answering
"no" to "Remind me at my next login?", choosing *Don't remind me*, or running
`bindle first-login dismiss` writes `~/.local/state/bindle/first-login-done`. `bindle first-login status` shows the
state; `bindle first-login remove [--purge]` uninstalls.

### Welcome apps

| Desktop | Welcome app | `bindle iso` (default) | Other installs |
|---|---|---|---|
| GNOME | Shell's "Welcome to …" dialog (→ GNOME Tour) | suppressed: GSettings override `welcome-dialog-last-shown-version='999'` | the dialog is drawn by the shell above all windows and isn't visible to other processes, so Bindle stays behind it and shows once it's closed |
| KDE Plasma | Welcome Center (KDED module) | suppressed: `/etc/xdg/plasma-welcomerc` `LastSeenVersion=99.0.0` (later Plasma upgrades still show the update page) | Bindle waits until it closes, then opens on top |
| Hyprland | `hyprland-welcome`, launched while the config is the autogenerated default | suppressed: each user's `~/.config/hypr/hyprland.lua` (and `/etc/skel`) is seeded with Hyprland's default minus the `autogenerated` line, the same thing the welcome app's *Finish* does (it also removes the yellow warning bar) | Bindle waits until it closes |
| COSMIC, i3 | initial setup / config wizard | kept (they configure, not welcome) | Bindle waits until they close |
| Enlightenment | first-run wizard inside the WM | kept | can't be detected; Bindle appears once it's done |

Opening *in front of* a welcome app isn't reliable: GNOME's dialog sits above
every window, and the others start at the same moment as Bindle. So Bindle
waits for the ones it can see, then opens on top and focused. `bindle iso
--keep-welcome` leaves all of them enabled;
`sudo bindle first-login quiet-welcome` applies the suppression on an existing system.

Three ways to get it onto a fresh install:

1. **archinstall config file**: `bindle first-login snippet --url <binary-url>` prints a
   `custom_commands` entry. archinstall runs it as root in `arch-chroot` after
   creating users and installing the desktop.
2. **From the live ISO** after archinstall finishes, before rebooting:
   `./bindle first-login install --root /mnt`.
3. **On the installed system**: `sudo bindle first-login install`.

XDG autostart covers the full desktops and Hyprland (via uwsm); the user
service covers Sway, niri, Labwc, i3, bspwm, Qtile, Xmonad and friends; the
console hook covers greeter-less setups like Awesome. Verified per desktop in
[VM-TEST.md](VM-TEST.md#results-archiso-20261001-archinstall-45-desktop-profile).
Exceptions: River shows no windows until the user writes `~/.config/river/init`,
and Enlightenment shows Bindle only after its own first-run wizard.

## Next steps

- Turn advisories into optional steps (remove unneeded GPU drivers; switch iwd → NetworkManager with the iwd backend).
- Per-desktop tuning beyond packages (e.g. GTK theming on Qt desktops and the reverse, Plasma/GNOME defaults).
- Track archinstall releases: re-run the extraction against new tags and diff `desktops.toml`.
