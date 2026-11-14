# Installing Arch with Bindle (live ISO)

Boot the Arch Linux ISO, connect to the internet, and run:

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/asafelobotomy/bindle/main/install.sh)
```

(`curl … | bash` works too; the script reattaches the wizard to the terminal.)
Downloading the script needs a connection: plug in a cable, or join Wi-Fi with
`iwctl station wlan0 connect "<network>"`. The installer saves that Wi-Fi in the
new system, so you are never asked again.

## `bindle install` (the default)

One wizard, in the same look and keys as the `bindle` wizard on the desktop
(also callable as `bindleinstall`):

| Step | What you choose |
|---|---|
| Network | Connected already, or a Wi-Fi network and its password (`iwctl` underneath). Saved into the new system. |
| Desktop | Desktop (uQ is recommended) and profile; the right pane shows what this hardware gets |
| Hardware | What Bindle recommends for the detected hardware, one row each: graphics driver, kernel (linux, linux-lts, linux-zen), boot loader (systemd-boot on UEFI, GRUB on BIOS; Limine on either), zram swap, power profiles, Bluetooth, audio. ←/→ changes a row, `r` goes back to the recommendation. CPU microcode is shown but set by archinstall |
| Disk | The disk (erased), Btrfs with Snapper snapshots or ext4, optional encryption (LUKS). `p` is manual partitioning: archinstall's own disk editor for this one step (keep another system, several disks, LVM), then back to the wizard; `x` returns to Bindle's layout |
| You | Computer name, user and password (with sudo), timezone, keyboard, language |
| Apps | Every app and setup step the profile and desktop bring, ticked; Space unticks one, `m` opens the bindle wizard to add more |
| Review | Everything above; `i` installs after a y/N confirmation |
| Install | One progress screen: the steps (disk, base system, boot loader, users and desktop, your apps and setup) with the latest output beside them, then Enter restarts |

Bindle then writes a complete archinstall config and credentials file to a private
directory and runs the ISO's own archinstall unattended (`--silent`) with Bindle's
plugin. archinstall partitions, installs the base system, bootloader, desktop and
user. The plugin carries the Wi-Fi over and sets the new system up before the
reboot (step 5 below): uQ, its login screen and every app in the profile. The
credentials file is deleted when archinstall returns. Bindle offers the reboot, and
the first boot lands on the desktop's login screen.

archinstall runs in a pseudo-terminal of its own (`script`, from util-linux) so the
progress screen can show its output; without `script` it prints straight to the
terminal as before. Everything it printed is kept in `/var/log/bindle-install.log`
on the ISO and copied to the same path in the new system. If the install fails,
the screen stays on the failed step with the last lines, and they are printed
again after you leave it.

Manual partitioning runs a small helper (`packaging/archinstall/bindle_disk_editor.py`,
embedded in the binary) with the ISO's Python. It opens archinstall's disk menu and
saves the layout in archinstall's own `disk_config` format, which the unattended
install then uses unchanged. Nothing is written to a disk before the confirmation
on Review. If this archinstall's menu can't be opened, the wizard says so and
`install.sh iso` remains the way to partition by hand.

`bindle install --dry-run` writes the files and prints the archinstall command.

## `bindle iso` (archinstall's own menu)

`install.sh iso` (or any options, as below) runs the older flow: a pre-install
screen, then archinstall's menu pre-filled with Bindle's preset, for setups the
wizard doesn't cover (custom partitioning, several disks, other kernels).

### What happens

```
install.sh ──► bindle iso ──► pre-install screen ──► archinstall --config <preset> --plugin <bindle>
                                (desktop + profile)        │ you set disks, users, hostname, locale
                                                           │ on_install hook: Bindle copied into /mnt,
                                                           │   first-login autostart, chosen profile
                                                           │ last hook: the profile applied in arch-chroot
                                                           ▼
                                                 reboot ──► first login ──► the desktop
                                                                           (Bindle opens only if a step failed)
```

1. **install.sh** checks it's on the live ISO (it refuses elsewhere because
   archinstall wipes disks), x86_64, root and online. It downloads the static
   `bindle-x86_64`, `SHA256SUMS` and `SHA256SUMS.sig` from one release (resolving
   "latest" to a tag first), checks the signature against the release key embedded
   in the script, checks the binary against `SHA256SUMS`, and only then starts
   `bindle iso`. Any mismatch stops it before anything runs.
2. **Pre-install screen**: pick a desktop (any of archinstall's 21, uQ, or "choose in
   archinstall") and a Bindle profile. The right pane shows what archinstall
   will be pre-filled with, based on the detected hardware. The recommended desktop
   is pre-selected. uQ has no archinstall profile: archinstall installs Hyprland with
   SDDM, and the selection adds the `uq` module, which Bindle applies before the reboot
   (step 5): it builds uQ from the AUR, runs `uqctl setup --apps` as you and switches
   to uQ's own login screen.
3. **archinstall** opens its normal menu with Bindle's preset loaded. You still set
   the disk layout, users, hostname, locale and timezone, and can change anything else.
4. **Bindle's archinstall plugin** runs at archinstall's `on_install` hook (base system
   installed, target mounted). It copies Bindle into the new system and runs
   `bindle first-login install --root <target>`. It also writes the chosen profile to
   `/etc/bindle/selection.toml` and a marker to `/etc/bindle/installed-by`. The
   Wi-Fi you joined on the ISO with `iwctl` is saved into the new system as a
   NetworkManager connection (`/etc/NetworkManager/system-connections/bindle-<name>.nmconnection`,
   root-only), so the first boot is online without asking again (`bindle carry-network`;
   WPA2/WPA3-transition and open networks; enterprise 802.1X ones aren't carried). Choosing
   "Reboot" in archinstall's final menu is fine; the plugin has already run.
   A failure in the plugin never aborts archinstall: it is logged (`[bindle] …`,
   also in `/var/log/archinstall/install.log`) and the install carries on;
   `bindle iso` then reports that Bindle wasn't set up and prints the command
   to finish it (`bindle first-login install --root /mnt`).
5. **Setting up the new system before the reboot.** At archinstall's last hook
   (users and the desktop exist), the plugin runs `bindle install-target --root <target>`.
   It runs `bindle apply --yes --from /etc/bindle/selection.toml` inside the new system
   (`arch-chroot`, as its first regular user, or `BINDLE_USER`), while the ISO still has
   the network: packages, config files, services, the AUR helper and AUR packages (uQ
   and its apps). sudo needs no password for that user while it runs (a drop-in in
   `/etc/sudoers.d/` that is removed afterwards, also on failure). Services are
   enabled, not started. User commands that fail there because they need a running
   session (such as `systemctl --user` or noti-fi's takeover) are saved to
   `~/.local/state/bindle/finish-at-login.jsonl` and run once at first login,
   without a window. If everything worked, setup is marked finished and the first
   login lands straight in the desktop; if a step failed, the install still completes
   and Bindle opens at first login to finish, as before. `--setup-at-first-login`
   skips this step and leaves everything to the first-login wizard.
6. The plugin also turns off the desktops' welcome apps (GNOME tour, KDE Welcome
   Center, Hyprland welcome), so Bindle is the welcome. `--keep-welcome` opts out;
   see [ARCHINSTALL.md](ARCHINSTALL.md#welcome-apps).
7. **First login**: the saved session steps run; Bindle opens only if setup isn't finished (desktop autostart or console hook), shows what
   was installed, and starts on the chosen profile, with archinstall's work marked ✓.

## Bindle's archinstall preset

| Setting | Value | Why |
|---|---|---|
| Desktop + greeter | your pick, with archinstall's default greeter | |
| Graphics driver | from detected GPUs (NVIDIA Turing and newer → open kernel module; older NVIDIA → nouveau, "All open-source" on hybrids; AMD+Intel → all open-source) | archinstall makes you choose blind; nvidia-open can't drive pre-Turing cards |
| Audio | PipeWire | |
| Network | NetworkManager | archinstall's iwd-only option leaves desktop network panels empty |
| Swap | zram, zstd | |
| pacman | colour, 10 parallel downloads | |
| Firewall / fonts | ufw; Noto + emoji + CJK, Liberation, DejaVu | except the minimal profile with no desktop |
| Bluetooth / power | enabled / power-profiles-daemon | only when the hardware is present / on laptops |
| multilib | enabled | gaming profile |

A `--config` file is deep-merged over the preset (its values win), except
`mirror_config.optional_repositories`, which is combined with the preset's list,
so adding `testing` keeps the gaming profile's `multilib`. The pre-install screen
summarises the merged result.

Values follow archinstall 4.5's config schema (`TESTED_ARCHINSTALL`); a unit
test round-trips the preset through Bindle's archinstall-config parser.

## Why not a fork of archinstall

archinstall already has supported extension points: `--config` pre-fills the
menu and `--plugin` adds install hooks. Using them means Bindle runs the
archinstall shipped on the ISO, which matches that ISO's kernel, keyring and
package set, and gets upstream fixes for free. A fork would have to track a
fast-moving project and be kept in sync with every monthly ISO. If Bindle ever
needs changes inside archinstall's own menus, contributing a hook upstream is
cheaper than maintaining a fork.

The plugin declares `__archinstall__version__ = 4.5`. Newer archinstall
releases log a "does not support" error for lower values but still load the
plugin. `bindle iso` prints a note whenever archinstall's major.minor differs from
`TESTED_ARCHINSTALL`, older as well as newer. Bump both after testing a new release.

## Options

```bash
bindle iso --desktop plasma --profile gaming      # pre-select on the screen
bindle iso --desktop gnome --profile desktop --yes # skip the screen
bindle iso --dry-run                               # write preset/plugin, print the command
bindle iso --keep-welcome                          # keep GNOME/KDE/Hyprland welcome apps
bindle iso --setup-at-first-login                  # apply the profile at first login, not before the reboot
bindle iso --config my.json                        # merge your archinstall config over the preset
bindle iso --workdir /root/bindle                  # write preset, plugin and selection here
bindle iso --silent --config my.json --creds creds.json --profile desktop --yes  # unattended
```

Unknown `--desktop` or `--profile` ids are rejected before anything runs
(profiles come from the catalog: `bindle list`). The preset, plugin and
selection are written to a fresh private directory (`/run/bindle-iso-…` as
root, mode 0700, files 0600), since a merged `--config` can hold your disk
layout and more. `--workdir <dir>` writes them to that directory instead (created if
missing; use it to inspect or keep the files, e.g. with `--dry-run`).
When archinstall returns, successfully or not, Bindle deletes the merged config and
`selection.toml` from the work directory (and removes the private directory
altogether); the plugin is left in a `--workdir`. `--dry-run` deletes nothing.

### Live-ISO check

`bindle iso` (without `--dry-run`) only runs when all three hold: `/run/archiso`
exists, the kernel command line has `archisobasedir=`, and `/` is the archiso
overlay (`airootfs`, per `/proc/self/mounts`). The error says which one failed.

### Unattended (`--silent`)

`--silent` needs `--config`, `--creds`, `--profile` and `--yes`, and the merged config
must have a non-empty `disk_config.device_modifications`. Every `device` listed there
must exist as a block device, and Bindle prints the target disks before starting
archinstall, since they can be erased.

`install.sh` passes its arguments through, and honours `BINDLE_VERSION`,
`BINDLE_REPO` and `BINDLE_BASE_URL` (mirrors and testing; non-https needs
`BINDLE_INSECURE=1`). Downloads from any of them must carry the release signature.

## Trust

Releases are signed. CI signs `SHA256SUMS` (covering `bindle-x86_64`,
`install.sh` and the AppImages) with the Bindle release key:

    75C4 0FA1 27FB F8D4 FBF9  E710 FAC4 0EFF E02D 69FC   Bindle release signing

The public key is `packaging/bindle-release.asc` and is embedded in
`install.sh`, which refuses a download whose signature or checksum doesn't
match. A compromised release would need the private key as well. To verify by
hand (for example the AppImage):

```bash
gpg --import packaging/bindle-release.asc
gpg --verify SHA256SUMS.sig SHA256SUMS
sha256sum -c --ignore-missing SHA256SUMS
```

`install.sh` itself is fetched from `main` over HTTPS; as with any
`curl | bash`, you can download it and read it first.

## Testing without an ISO

- `BINDLE_FORCE=1 BINDLE_INSECURE=1 BINDLE_BASE_URL=file:///path/to/release bash install.sh --dry-run`
  exercises download, verification and handoff. The path needs `bindle-x86_64`,
  `SHA256SUMS` and a `SHA256SUMS.sig` by the embedded key; `tests/vm/run.sh` shows
  how to test with a throwaway key instead (it serves a copy of `install.sh`
  embedding that key).
- `bindle iso --dry-run` writes the preset, plugin and selection without running archinstall.
- The plugin can be driven with a mock `installation` object whose `target` is a temp directory.
- Before a release: one full install in a VM from the current ISO.
