# Installing Arch with Bindle (live ISO)

Boot the Arch Linux ISO, connect to the internet, and run:

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/asafelobotomy/bindle/main/install.sh)
```

(`curl … | bash` works too; the script reattaches the wizard to the terminal.)
Wi-Fi first if needed: `iwctl station wlan0 connect "<network>"`.

## What happens

```
install.sh ──► bindle iso ──► pre-install screen ──► archinstall --config <preset> --plugin <bindle>
                                (desktop + profile)        │ you set disks, users, hostname, locale
                                                           │ on_install hook: Bindle copied into /mnt,
                                                           │   first-login autostart, chosen profile
                                                           ▼
                                                 reboot ──► first login ──► Bindle wizard
                                                                           (profile pre-selected)
```

1. **install.sh** checks it's on the live ISO (it refuses elsewhere because
   archinstall wipes disks), x86_64, root and online. It downloads the static
   `bindle-x86_64`, `SHA256SUMS` and `SHA256SUMS.sig` from one release (resolving
   "latest" to a tag first), checks the signature against the release key embedded
   in the script, checks the binary against `SHA256SUMS`, and only then starts
   `bindle iso`. Any mismatch stops it before anything runs.
2. **Pre-install screen**: pick a desktop (any of archinstall's 21, or "choose in
   archinstall") and a Bindle profile. The right pane shows what archinstall
   will be pre-filled with, based on the detected hardware.
3. **archinstall** opens its normal menu with Bindle's preset loaded. You still set
   the disk layout, users, hostname, locale and timezone, and can change anything else.
4. **Bindle's archinstall plugin** runs at archinstall's `on_install` hook (base system
   installed, target mounted). It copies Bindle into the new system and runs
   `bindle first-login install --root <target>`. It also writes the chosen profile to
   `/etc/bindle/selection.toml` and a marker to `/etc/bindle/installed-by`. Choosing
   "Reboot" in archinstall's final menu is fine; the plugin has already run.
   A failure in the plugin never aborts archinstall: it is logged (`[bindle] …`,
   also in `/var/log/archinstall/install.log`) and the install carries on;
   `bindle iso` then reports that Bindle wasn't set up and prints the command
   to finish it (`bindle first-login install --root /mnt`).
5. The plugin also turns off the desktops' welcome apps (GNOME tour, KDE Welcome
   Center, Hyprland welcome), so Bindle is the welcome. `--keep-welcome` opts out;
   see [ARCHINSTALL.md](ARCHINSTALL.md#welcome-apps).
6. **First login**: Bindle opens (desktop autostart or console hook), shows what
   was installed, and starts on the chosen profile, with archinstall's work marked ✓.

## Bindle's archinstall preset

| Setting | Value | Why |
|---|---|---|
| Desktop + greeter | your pick, with archinstall's default greeter | |
| Graphics driver | from detected GPUs (NVIDIA → open kernel module; AMD+Intel → all open-source) | archinstall makes you choose blind |
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
