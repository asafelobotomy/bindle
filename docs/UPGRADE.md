# bindleupgrade: update everything in one run

```bash
bindleupgrade                 # guided: shows each stage, asks before the big ones
bindleupgrade --check         # report what is available; change nothing, no password
bindleupgrade --check --json  # the same plan as JSON
bindleupgrade --yes           # unattended
bindleupgrade --skip aur,flatpak
bindleupgrade --only pacman
bindleupgrade -n              # print every command, run none
bindleupgrade --prefs         # the preferences in effect, as a file to save
bindo upgrade-prefs           # edit ~/.config/bindle/upgrade.toml
bindo update                  # the same, from bindo
```

Also `bindle upgrade`. `bindleupgrade` is a link to the `bindle` binary
(see [BINDO.md](BINDO.md#one-binary-three-names)).

## Stages

In order; each can be left out with `--skip <id>` or picked with `--only`.

| id | Stage | What it does |
|---|---|---|
| `bindle` | Bindle | The latest release of asafelobotomy/bindle against the running version. Newer: asked (default yes), then downloaded and verified like `install.sh` does (`SHA256SUMS` must carry a valid `gpgv` signature by the release key's fingerprint `75C40FA1…E02D69FC`, built in from `packaging/bindle-release.asc`; each file must match its line there). The release's `VERSION` file, listed in that signed `SHA256SUMS`, must hold exactly the tag being installed, and that tag must be newer than the running Bindle: a signed older release served as the latest is refused, as is a tag from the redirect that isn't a version (`v1.2.3` or `v1.2.3-rc.1`). The install `first-login install` made (`/usr/local/bin/bindle`, and `bindle-gui` beside it) is replaced through the root helper, which checks each file's sha256 again on its own root-owned copy and writes only those two paths; a user-owned AppImage is replaced in place. The run then stops and starts the new Bindle with the same arguments (and `BINDLE_UPGRADED=1`, so it doesn't update itself again): new pins, modules and checks apply to the rest. Run from anywhere else (a build tree, a package): reported only. |
| `news` | Arch news | Posts on archlinux.org/news since the last full upgrade (from `/var/log/pacman.log`; without one, the last `news_days`, 30 by default). A post that asks for manual intervention is asked about first (default no); with `--yes` the package stages are held instead, unless `--ignore-news`. Unreadable feed: noted, not fatal. |
| `preflight` | Before updating | Holds the package stages, with the reason, when: `/` has less free space than `space.root_mb` (3072 MB); with a kernel, microcode, driver or mkinitcpio update, a separate `/boot` has less than `space.boot_mb` (200 MB) or the EFI partition (FAT at /efi, /boot/efi or /boot) less than the larger of `boot_mb` and `efi_mb` (100 MB) — a full /boot is the classic unbootable kernel upgrade, and linux-zen with NVIDIA's DKMS modules makes large images; Hyprland in the repo updates is outside uq-hyprland-pin's range (`hyprland>=… hyprland<…` from its dependencies), which would make pacman refuse the whole upgrade (with uQ's new pin in the AUR updates, it says to install the pin and Hyprland in one transaction); on battery with a kernel or driver update, `battery_max_packages` (30) or more packages, or less than `battery_min_percent` (40%): asked (default no), held under `--yes` unless `--on-battery`. |
| `snapshot` | Snapshot | `snapper -c root create` before upgrading, when snapper has a root config and there are package upgrades. Skipped when `snap-pac` is installed (it snapshots every pacman run). A failed snapshot asks whether to go on. |
| `mirrors` | Mirrors | `systemctl start reflector.service` (reflector's own settings) when the mirror list is `mirror_max_age_days` old (14 by default) and reflector is installed. |
| `pacman` | Packages | Lists the updates from `checkupdates` (pacman-contrib: a private copy of the databases, no root), then `pacman -Syu` on your terminal: pacman shows its own summary and asks its own questions (proceed, replacements, conflicts). Unattended (`--yes`, or no terminal) it is asked here and runs with `--noconfirm`, which takes pacman's default answers; a conflict that defaults to "no" fails the stage with a hint to run it from a terminal. A new `archlinux-keyring` is installed first. Without pacman-contrib it still upgrades. |
| `aur` | AUR | `paru -Sua` (or `yay`) as the user, with its own PKGBUILD review; `--yes` skips the review. Held when the system upgrade failed. |
| `releases` | Bindle apps | The catalog's pinned GitHub releases (`[[module.release]]`) that are newer than what is installed (`vercmp`; AppImages by the asset their desktop entry names). Only apps Bindle installed from its releases are updated; a copy from the Arch repos, the AUR or elsewhere stays on that source (see [Preferences](#preferences)). Downloaded and checked against the pinned sha256 as the user, packages installed through the helper. |
| `flatpak` | Flatpak | `flatpak update -y --noninteractive` when `flatpak remote-ls --updates` lists anything, then `flatpak uninstall --unused` (asked, default yes). |
| `firmware` | Firmware | `fwupdmgr get-updates`. A real run first refreshes fwupd's list (`fwupdmgr refresh`) when it is `firmware_refresh_days` (7) old; `--check`, the timer and the window only look, and say how old the list is. Reported only; `--firmware` installs (`fwupdmgr update`). |
| `rebuild` | Rebuilds | After the upgrade, `checkrebuild` (rebuild-detector, which Bindle's AUR-helper module now installs) lists packages a library update (Qt, Python, ICU…) broke; the foreign (AUR) ones are rebuilt with `paru -S --rebuild` (asked, default yes; in the window, handed to a terminal). Without rebuild-detector it says to install it. |
| `uq` | uQ | When `uq` was upgraded: `uqctl setup --apps --json` as the user (it never overwrites the user's own files; each file it wrote or kept is reported), and it says to log out and back in: uqd and the shell keep running the old version until then (`uqctl reload` isn't used: it would have the old uqd render the new templates). uqctl has no migrate or post-upgrade command; asafelobotomy/uq#57 makes `uq` depend on the pin it is tested with, adds a log-in-again note to the package, and suggests one. If greetd (uQ's login screen) was the display manager before the run and isn't after, `uqctl greeter enable` as root. |
| `pacnew` | Config files | `.pacnew` / `.pacsave` files (`pacdiff --output`, else a search of /etc), looked at after the upgrade. Reported; `bindo pacnew` merges them. |
| `orphans` | Orphans | `pacman -Qdtq`, after the upgrade. Asked about (default no); `--yes` only reports; `--remove-orphans` removes. |
| `services` | Services | `systemctl --failed` (system and user) before and after; units that newly failed are reported with `systemctl status`. |
| `setup` | Bindle setup | What's new ([UPGRADE-PATH.md](UPGRADE-PATH.md)): what this Bindle would add, change or retire compared with earlier applies, reviewed with `bindle whats-new` or the window's What's new button; after Bindle updated itself, offered right after the run. Also files Bindle wrote that changed by hand since (for modules whose catalog content didn't change), and drives in Bindle's fstab block that aren't connected. Reported only. |
| `reboot` | Reboot | Recommended when the running kernel's modules are gone (a kernel upgrade), or a kernel, microcode, NVIDIA driver, Mesa, systemd, glibc or D-Bus was upgraded. "Don't restart yet" when a DKMS module (`dkms status`) isn't built for an installed kernel, or a kernel's `vmlinuz-`/`initramfs-` image is missing from /boot. After an upgrade it offers to fix it (asked, default yes): `dkms autoinstall -k <version>` for each kernel, then `mkinitcpio -P`, as root; until that succeeds the window shows no Restart button. `--check` reports it with the commands. /boot is often root-only: a run that started the root helper has it list /boot's file names; otherwise the images aren't checked, and it says so. |

Each stage prints `==> [n/N] Title: summary` and its details (up to 12
lines). The run ends with one row per stage: done, current, note, skipped,
held or failed. Exit status 1 when a stage failed.

## Preferences

`/etc/bindle/upgrade.toml` (the system's), then `~/.config/bindle/upgrade.toml`
(yours); each value comes from the last file that sets it. A broken file is
skipped with a warning. `bindleupgrade --prefs` prints the values in effect
as a file; `bindo upgrade-prefs` writes yours from that and opens it.

```toml
mirror_max_age_days = 14   # refresh the mirror list when it is this old
news_days = 30             # news shown when pacman.log has no past upgrade
firmware_refresh_days = 7  # refresh fwupd's metadata when it is this old
battery_min_percent = 40   # on battery: below this, package upgrades wait
battery_max_packages = 30  # on battery: this many packages (or a kernel) wait too

[space]                    # free MB the package stages need
root_mb = 3072
boot_mb = 200
efi_mb = 100

[releases]
follow = "source"          # or "bindle"

[releases.apps]
kyko = "bindle"            # per app, over `follow`
```

**Where a pinned app updates from.** Each installed app Bindle pins is
classified by where its copy came from:

- *Bindle's release*: Bindle installed it. Apply records release packages in
  its snapshot (`releases` in the manifest); snapshots from before that
  listed them among `packages`, and a package there that no repo has
  (`pacman -Qm`) counts too. AppImages are Bindle's when they carry Bindle's
  desktop entry.
- *the Arch repos*: pacman knows the package from a sync database.
- *the AUR or a manual install*: a foreign package Bindle has no record of.

With `follow = "source"` (the default) only Bindle's own copies are updated;
the others are listed as kept, with the line that would switch them. With
`"bindle"`, globally or per app, Bindle's newer release replaces the copy
(`pacman -U`). Flatpak copies are separate installs and never touched.
`bindle apply` already leaves an app alone when any copy of it is installed.

## Checks in the background

`bindo update-notify on` installs a user timer (`~/.config/systemd/user/bindle-update-check.timer`: daily and 15 minutes after login) that runs `bindleupgrade --check --notify`. When there is work, a notification (through the desktop's notification server: on uQ, noti-fi) lists it with Update and Later; Update opens `bindleupgrade --gui`, or bindleupgrade in a terminal where the window can't start. `bindo update-notify off` removes the timer.

## The window

`bindleupgrade --gui` opens the same run in Bindle's desktop look
(`bindle-gui --upgrade`, see [GUI.md](GUI.md#the-update-window)): a Check page
with a switch per stage, then the run and a result table with Restart, the
AUR in a terminal, and the log. `-n` and `--sandbox` work there too.

## Design

### Its own command, built like `bindle apply`

`bindleupgrade` is a `bindle` subcommand with its own name, not a bindo
recipe: it needs a pure plan, typed stages, a result table and the root
helper, which a shell recipe can't give cleanly. bindo's `update` recipe
calls it.

The `bindle-upgrade` crate keeps the three parts apart:

1. `probe` looks at the system as the user and fills a `Probe` (nothing
   needs root); `parse`, `system` and `news` turn command output into data.
2. `plan::plan(&Probe, &Options)` is pure: it decides each stage's
   `Todo` (`Run(actions)`, `Report`, `Nothing`, `Skip`, `Hold`), its
   question and what declining it stops. Every rule is a unit test.
3. `run` carries it out. Root actions go to one session of Bindle's root
   helper (`RunAsRoot` steps and `InstallFiles`), started when the first one
   is due: one password prompt (pkexec, like every approval in Bindle),
   logged to `/var/log/bindle/upgrade-*.log`, and the helper's lock keeps it
   from running beside `bindle apply`. `pacman -Syu` goes to the helper as an
   `interactive` request naming your terminal (`/dev/pts/N`): the helper
   checks it is a terminal device owned by the user who authenticated
   (`PKEXEC_UID`) and runs pacman on it. AUR helpers, Flatpak and fwupd run as
   the user with the terminal; they ask for their own permissions as they
   always do.

The closing stages (`rebuild`, `uq`, `pacnew`, `orphans`, `services`,
`setup`, `reboot`; `plan::after_plans`) are planned again from a fresh look
after the upgrades, with the list of packages that were upgraded.

### Failures

A failed stage doesn't stop the run. What depends on it does: no AUR builds
or release installs on top of a failed `pacman -Syu` (partial upgrades break
things), and a failed snapshot asks before upgrading without one.

### Rehearsals

- `--dry-run` / `-n`: the real look at the system, then every command printed
  (`would run as root: …`, `would run: …`, `would download …`), nothing run,
  no helper started.
- `--sandbox <dir>` (hidden, like `bindle apply --sandbox`): root commands go
  through the helper in `--fake-commands` mode, which records them in
  `<dir>/var/log/bindle/upgrade-*.log`. Exercises the protocol unprivileged.

Never test a real upgrade on the host; use the Arch VM (`tests/vm/run.sh`,
[VM-TEST.md](VM-TEST.md)).

## Not done (choices left open)

- When Hyprland outgrows uQ's pin and the new pin is on the AUR, Bindle holds the upgrade and says
  to install the pin and Hyprland in one transaction; it doesn't do that itself (not verified in
  the VM which paru invocation does it safely).
- Bindle's self-update covers `/usr/local/bin` and AppImages; a distribution package of Bindle
  would be updated by its package manager.
- No `--json` for the closing table.
- No preferences page in the GUI or uQ Settings yet: the file is the preference
  (`bindo upgrade-prefs` opens it).
