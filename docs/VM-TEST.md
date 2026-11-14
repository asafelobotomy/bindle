# End-to-end VM test

`tests/vm/run.sh` installs Arch in a QEMU/KVM VM through the `bindle iso` path
(`install.sh` with options → `bindle iso` → archinstall; `install.sh` with no
arguments runs `bindle install`, which this test doesn't cover), boots the result, runs a real `bindle apply`, and screenshots the first-login
wizard. No clicks.

## Requirements

`qemu-system-x86`, `edk2-ovmf`, `socat`, `bsdtar`, `python3`, a writable
`/dev/kvm`, about 25 GB free disk (sparse) and 2–2.5 GB free RAM.

## Run

```bash
mkdir -p tests/vm/.work && cd tests/vm/.work
curl -fsSLO https://archlinux.org/iso/latest/sha256sums.txt
curl -fLO https://geo.mirror.pkgbuild.com/iso/latest/archlinux-x86_64.iso
grep ' archlinux-x86_64.iso' sha256sums.txt | sha256sum -c -
cd ../../..
tests/vm/run.sh            # install + boot; or `install` / `boot` separately
DESKTOP=plasma PROFILE=gaming tests/vm/run.sh
```

Outputs in `tests/vm/.work/` (git-ignored): `install-serial.log`,
`boot-serial.log`, `shots/*.png`, `disk.raw`.

## How it works

1. **Install**: the ISO's kernel and initramfs boot directly (UEFI via OVMF), with
   archiso's `script=` boot parameter pointing at `guest/guest-install.sh` on a local
   HTTP server (the host is `10.0.2.2` inside QEMU's user network). That script runs
   `install.sh` with `BINDLE_BASE_URL` set to the local server, so it downloads the
   freshly built binary and checksum exactly as a user would from a release:
   `bindle iso --desktop … --profile … --yes --silent --config guest/archinstall-test.json --creds guest/creds-test.json`.
   The test config adds only what an unattended archinstall needs: disk layout
   (`/dev/vda`, ESP + ext4), hostname, locale, bootloader. Bindle's preset supplies the rest.
2. **Test-only additions** to the new system: a first-boot check service, a
   test-only polkit rule that lets the test user approve Bindle's actions without
   a password, and GDM autologin.
3. **Boot**: the check service (before GDM) runs, as `tester`: `bindle archinstall`,
   `first-login status`, `plan`, a real `apply` (approved through that polkit rule), `undo --list`
   and a second `plan`. Results stream to the serial log between `BINDLE-VM:` markers.
4. GDM auto-logs in; the XDG autostart entry should open the first-login wizard
   in GNOME Console. Screenshots every 30 s via the QEMU monitor.

Re-running only the boot phase (`tests/vm/run.sh boot`) restores the saved fresh
install (`disk.installed.raw`, a reflink copy on btrfs) and the guest pulls the
host's latest build first, so code changes don't need a 25-minute reinstall.

Scenario switches (env vars for `run.sh`):

| Switch | Phase | Effect |
|---|---|---|
| `KEEP_WELCOME=1` | install | `bindle iso --keep-welcome` |
| `SETUP_TARGET=1` | install | set the profile up inside the new system before the reboot (what users get); without it the test passes `--setup-at-first-login` to keep checking the first-login wizard |
| `CLOSE_WELCOME=1` | boot | Alt+F4 via the QEMU monitor after the third screenshot |
| `SECOND_LOGIN=1` | boot | pre-sets `first-login-seen` for the test user, so login shows the reminder notification |
| `WITH="uq scdr crumbs noti-fi"` | boot | adds those modules to the check's `plan` and `apply` (`--with`); needs an install made with this switch's support |

## All desktops

```bash
tests/vm/matrix.sh                         # every archinstall desktop, one VM at a time
DESKTOPS="plasma sway" tests/vm/matrix.sh  # a subset
```

Each desktop runs `run.sh all`; results land in `.work/results/<desktop>/`
(serial logs, screenshots) and `.work/results/summary.tsv`. A caching pacman
mirror (`pkgproxy.py`, port 8766) makes repeat runs ~8 minutes each. The guest
auto-logs in through whichever login manager archinstall installed (GDM, SDDM,
Plasma Login Manager, LightDM, greetd/COSMIC, or console autologin when there
is none), and `bindle-vm-diag.service` dumps sessions, processes and the
first-login watcher's status to the serial log two minutes after login.

### Results (archiso 2026.10.01, archinstall 4.5, `desktop` profile)

| Desktop | Install | Real apply | First login | Notes |
|---|---|---|---|---|
| KDE Plasma | ✅ | ✅ | ✅ Konsole (autostart) | KDE Welcome Center opens on top |
| GNOME | ✅ | ✅ | ✅ GNOME Console (autostart) | GNOME tour opens on top; exactly one Bindle (launch lock) |
| Cosmic | ✅ | ✅ | ✅ COSMIC Terminal (autostart) | |
| Xfce4 | ✅ | ✅ | ✅ xfce4-terminal | |
| Cinnamon | ✅ | ✅ | ✅ GNOME Terminal | |
| Mate | ✅ | ✅ | ✅ MATE Terminal | |
| Budgie | ✅ | ✅ | ✅ | |
| Lxqt | ✅ | ✅ | ✅ QTerminal | |
| Deepin | ✅ | ✅ | ✅ Deepin Terminal | |
| Enlightenment | ✅ | ✅ | ⚠️ running, behind Enlightenment's own first-run wizard | shows after the wizard |
| Hyprland | ✅ | ✅ | ✅ kitty (uwsm autostart) | Hyprland welcome opens on top |
| Sway | ✅ | ✅ | ✅ foot (user-service watcher) | |
| niri | ✅ | ✅ | ✅ Alacritty (watcher) | screen black in the VM: niri needs a GPU render node |
| Labwc | ✅ | ✅ | ✅ Alacritty (watcher) | |
| River | ✅ | ✅ | ⚠️ running, invisible | archinstall's River has no init/layout, so no window is ever shown |
| i3-wm | ✅ | ✅ | ✅ xterm (watcher) | i3's config prompt floats on top |
| Awesome | ✅ | ✅ | ✅ console (no greeter; profile.d hook) | |
| Bspwm | ✅ | ✅ | ✅ urxvt (watcher) | |
| Qtile | ✅ | ✅ | ✅ Alacritty (watcher) | |
| Xmonad | ✅ | ✅ | ✅ xterm (watcher) | |
| niri + DankMaterialShell | ❌ upstream | — | — | archinstall 4.5 installs `dms-shell-niri`/`dms-greeter`, not in the repos; `bindle iso` marks it unavailable |

After every apply, a second `bindle plan` leaves only the always-run user command.

## What it has caught

| Finding | Fix |
|---|---|
| archiso's initramfs didn't find the ISO attached as an IDE CD-ROM | harness attaches it as a read-only virtio disk |
| Fastly mirror stalled mid-pacstrap ("less than 1 byte/sec") | harness pins mirrors via `custom_servers` (not Bindle) |
| `mirror_regions` names must be real archinstall regions (`KeyError`) | use `custom_servers` for custom mirrors |
| **Bindle:** `sudo -v` demands a password unless *every* sudoers entry is NOPASSWD, so unattended applies failed although the commands themselves needed none | credential check is now `sudo true` |
| **Bindle:** AUR helper detection only matched `paru`/`yay`, not `paru-bin` (what Bindle itself installs), so re-runs re-bootstrapped paru | match `-bin`/`-git` variants |
| **Bindle:** the console-login hook ran inside greetd's `sh -l -c` session start, so COSMIC never started (Bindle took over the VT) | hook only runs in interactive shells |
| **Bindle:** window managers without XDG autostart (Sway, niri, i3, bspwm, Qtile, Xmonad, Labwc, River) never opened Bindle | systemd user service waits for this user's graphical session (logind + display socket), then opens the terminal |
| **Bindle:** that service's terminal was killed the moment the watcher exited (`KillMode=control-group`) | `KillMode=process` |
| **Bindle:** the watcher gave up after 3 min, so a console/SSH login first (or a slow first boot) skipped it | waits indefinitely; exits once the marker exists |
| **Bindle:** greetd sessions report logind type `tty`, so the watcher ignored them | any active session counts for (per-user) Wayland sockets; X11 still needs an x11 session |
| **Bindle:** Budgie ships labwc, which was detected as a second desktop and got labwc's bar/launcher installed | desktops declare `includes` |
| **Bindle:** pacman groups (`deepin-extra`) never counted as installed and were re-planned forever | groups resolved via `pacman -Sgg` |
| **Bindle:** xterm/urxvt bitmap fonts lack ✓ • ↑↓ | launched with a scalable monospace font |
| **Upstream:** archinstall 4.5 niri + DankMaterialShell profile can't install | marked `broken`; `bindle iso` refuses it |
| harness: agetty autologin without `-p -f` asked for a password | fixed |
| **UX:** desktop welcome apps (GNOME tour, KDE Welcome Center, Hyprland welcome, Enlightenment wizard, i3 config prompt) open on top of Bindle | resolved: `bindle iso` suppresses them; otherwise Bindle waits for them to close (see below) |
| **UX:** first-login terminal is 80×24 (GNOME Console); summary lines clipped | shorter summary lines; narrow layout for the module picker |

## Last result (archiso 2026.11.01, archinstall 4.5, GNOME, `desktop` profile)

- Install: `install.sh` (signature and checksum verified) → `bindle iso --silent` → archinstall with Bindle's preset; the plugin installed Bindle into the target. **OK**
- First boot: `bindle archinstall` read archinstall's logged config (exact); PipeWire, fonts and ufw shown as done.
- Real `bindle apply`: 10/10 steps, including a full upgrade with 106 packages, unit enables and paru-bin built from the pinned AUR commit; snapshot and log written. **OK**
- Second `plan`: "Nothing to do: this system already matches the selection."
- GDM autologin → `bindle first-login run` starts the desktop window (`bindle-gui --first-login`) on its own, with no terminal and no GNOME welcome dialog; `install.sh` had verified and installed it, and `bindle iso` added `kirigami` and `qt6-declarative`. **OK** (re-run after the window shipped)
- Not covered: key presses from the QEMU monitor reach GNOME Shell but not the unfocused Console window, so the screens after the first are checked by the 80×24 render tests and a pty-driven run on the host instead.

### Welcome apps and reminders (archiso 2026.10.01)

| Scenario | Result |
|---|---|
| GNOME via `bindle iso` | ✅ no "Welcome to Arch Linux" dialog; Bindle in GNOME Console |
| Plasma via `bindle iso` | ✅ no Welcome Center; Bindle in Konsole |
| Hyprland via `bindle iso` | ✅ no hyprland-welcome and no "autogenerated config" bar (config seeded); Bindle in kitty |
| Plasma with `--keep-welcome` (`KEEP_WELCOME=1 CLOSE_WELCOME=1`) | ✅ Welcome Center alone first; Bindle waits, then opens on top after Alt+F4 |
| Second login, GNOME (`SECOND_LOGIN=1`) | ✅ notification "Finish setting up your system" instead of the wizard |
| Second login, Plasma | ✅ notification with *Set up now* / *Don't remind me* buttons |
| Second login, Sway (mako) | ✅ notification; clicking it opens the wizard |
