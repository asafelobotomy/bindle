# bindleUP: update everything in one run

```bash
bindleup                 # asks once what to update, then the password once, then runs by itself
bindleup --check         # report what is available; change nothing, no password
bindleup --check --json  # the same plan as JSON
bindleup --yes           # unattended
bindleup --skip aur,flatpak
bindleup --only pacman
bindleup -n              # print every command, run none
bindleup --prefs         # the preferences in effect, as a file to save
bindleup --dry-run --json   # what a run would change, as one plan document
bindleup --check --ndjson   # the look at the system as JSON lines, then the plan
bindleup --ndjson           # an unattended run as JSON lines (see below)
bindle config show      # the preferences as typed keys; `config set <key> <value>`, `config reset <key>`
bindo upgrade-prefs           # edit ~/.config/bindle/upgrade.toml
bindo update                  # the same, from bindo
```

Also `bindle upgrade`. `bindleup` is a link to the `bindle` binary
(see [BINDO.md](BINDO.md#one-binary-three-names)).

## Stages

In order; each can be left out with `--skip <id>` or picked with `--only`.

| id | Stage | What it does |
|---|---|---|
| `bindle` | Bindle | The latest release of asafelobotomy/bindle against the running version. Newer: asked (default yes), then downloaded and verified like `install.sh` does (`SHA256SUMS` must carry a valid `gpgv` signature by the release key's fingerprint (`CCFF34EE…DE41DF9C`; `75C40FA1…E02D69FC` signed up to v0.1.9 and is no longer trusted for updates), built in from `packaging/bindle-release.asc`; each file must match its line there). The release's `VERSION` file, listed in that signed `SHA256SUMS`, must hold exactly the tag being installed, and that tag must be newer than the running Bindle: a signed older release served as the latest is refused, as is a tag from the redirect that isn't a version (`v1.2.3` or `v1.2.3-rc.1`). The install `first-login install` made (`/usr/local/bin/bindle`, and `bindle-gui` beside it) is replaced through the root helper, which checks each file's sha256 again on its own root-owned copy and writes only those two paths; a user-owned AppImage is replaced in place. The run then stops and starts the new Bindle with the same arguments (and `BINDLE_UPGRADED=1`, so it doesn't update itself again): new pins, modules and checks apply to the rest. Run from anywhere else (a build tree, a package): reported only. |
| `news` | Arch news | Posts on archlinux.org/news since the last full upgrade (from `/var/log/pacman.log`; without one, the last `news_days`, 30 by default). A post that asks for manual intervention is asked about first (default no); with `--yes` the package stages are held instead, unless `--ignore-news`. Unreadable feed: noted, not fatal. |
| `preflight` | Before updating | Holds the package stages, with the reason, when: `/` has less free space than `space.root_mb` (3072 MB); with a kernel, microcode, driver or mkinitcpio update, a separate `/boot` has less than `space.boot_mb` (200 MB) or the EFI partition (FAT at /efi, /boot/efi or /boot) less than the larger of `boot_mb` and `efi_mb` (100 MB) — a full /boot is the classic unbootable kernel upgrade, and linux-zen with NVIDIA's DKMS modules makes large images; Hyprland in the repo updates is outside uq-hyprland-pin's range (`hyprland>=… hyprland<…` from its dependencies), which would make pacman refuse the whole upgrade (with uQ's new pin in the AUR updates, it says to install the pin and Hyprland in one transaction); on battery with a kernel or driver update, `battery_max_packages` (30) or more packages, or less than `battery_min_percent` (40%): asked (default no), held under `--yes` unless `--on-battery`. |
| `snapshot` | Snapshot | One snapper pair for the whole run, when snapper has a root config and there are package upgrades: a `pre` snapshot described `bindleup` before the packages, and its `post` after the last stage (the root helper takes it as it exits if the run is cut short). With `snap-pac` installed its own snapshots are skipped for the run's pacman calls (`SNAP_PAC_SKIP=y` in the helper), so a run is one restore point instead of a pair per pacman call and AUR install. A failed snapshot asks whether to go on. |
| `mirrors` | Mirrors | `systemctl start reflector.service` (reflector's own settings) when the mirror list is `mirror_max_age_days` old (14 by default) and reflector is installed. |
| `pacman` | Packages | Lists the updates from `checkupdates` (pacman-contrib: a private copy of the databases, no root), then `pacman -Syu` on your terminal: pacman shows its own summary and asks its own questions (proceed, replacements, conflicts). Unattended (`--yes`, or no terminal) it is asked here and runs with `--noconfirm`, which takes pacman's default answers; a conflict that defaults to "no" fails the stage with a hint to run it from a terminal. A new `archlinux-keyring` is installed first. Without pacman-contrib it still upgrades. |
| `aur` | AUR | `paru -Sua --noconfirm --skipreview` (or yay's equivalent) as the user: no build-file review, in the terminal, the window and uQ Settings alike; without a terminal paru's output comes line by line. Its `sudo pacman` calls go through the run's one root helper: no password. Held when the system upgrade failed. When paru fails, the stage's result names the packages that didn't upgrade (`not upgraded: kyko 0.2.2-1 → 1.3.2-1`) and, without a terminal, paru's and makepkg's error lines; the ones it did upgrade before failing count as upgraded, so uQ's setup and the reboot check still run for them. **Partner versions:** first it reads each update's new `conflicts` and `depends` from the AUR's info API and the installed AUR packages' (`pacman -Qmi`). An update whose versioned relations can't be met by what will be installed waits (`--ignore`, named as `held back: hauler 0.2.6-1 doesn't work with oysterav 0.5.0-1 (oysterav<0.6.0); it waits for oysterav 0.6.0`), and so does an update that would break an installed app that has no update for it yet; the rest still update. When one update needs another's new version, they go in runs in that order (`paru -S --needed -- uq`, then `-- hauler`), and two that need each other in one `--batchinstall` transaction. uQ and its apps declare these versions from uQ's `compat/family.toml`. If the AUR can't be asked, nothing is held and pacman still refuses a set that clashes. |
| `releases` | Bindle apps | The catalog's pinned GitHub releases (`[[module.release]]`) that are newer than what is installed (`vercmp`; AppImages by the asset their desktop entry names). Only apps Bindle installed from its releases are updated; a copy from the Arch repos, the AUR or elsewhere stays on that source (see [Preferences](#preferences)). Downloaded and checked against the pinned sha256 as the user, packages installed through the helper. Also Bindle's old AppImage copies (`~/.local/bin/<app>` and `bindle-<app>.desktop`, from before its apps moved to the AUR) once a package installs `/usr/bin/<app>`, or launchers whose AppImage is gone: removed as the user, asked (default yes). A copy no package replaces stays. |
| `flatpak` | Flatpak | `flatpak update --user …` as you and `flatpak update --system …` through the root helper (no polkit prompt), when `flatpak remote-ls --updates` lists anything, then `flatpak uninstall --unused` (asked, default yes). |
| `firmware` | Firmware | `fwupdmgr get-updates`. A real run first refreshes fwupd's list (`fwupdmgr refresh`) when it is `firmware_refresh_days` (7) old; `--check`, the timer and the window only look, and say how old the list is. Reported only; `--firmware` installs (`fwupdmgr update`). |
| `rebuild` | Rebuilds | After the upgrade, `checkrebuild` (rebuild-detector, which Bindle's AUR-helper module now installs) lists packages a library update (Qt, Python, ICU…) broke; the foreign (AUR) ones are rebuilt with `paru -S --rebuild --noconfirm --skipreview`, named in the stage's summary, with no question and no terminal needed (the window and uQ Settings rebuild too). Afterwards it looks again: a package the rebuild didn't clear (it ships prebuilt files, so the flag stays) is remembered at that version in `~/.local/state/bindle/rebuild-stuck.json` and not rebuilt again until a new version of it is installed; it is listed with `checkrebuild -v` to see why. The stages after it are then planned again from that look, so Reboot checks the modules and boot images the rebuilds left. |
| `uq` | uQ | When `uq` was upgraded: `uqctl setup --apps --json` as the user (it never overwrites the user's own files; each file it wrote or kept is reported), and it says to log out and back in: uqd and the shell keep running the old version until then (`uqctl reload` isn't used: it would have the old uqd render the new templates). uqctl has no migrate or post-upgrade command; asafelobotomy/uq#57 makes `uq` depend on the pin it is tested with, adds a log-in-again note to the package, and suggests one. If greetd (uQ's login screen) was the display manager before the run and isn't after, `uqctl greeter enable` as root. |
| `pacnew` | Config files | `.pacnew` / `.pacsave` files (`pacdiff --output`, else a search of /etc), looked at after the upgrade. Reported; `bindo pacnew` merges them. |
| `orphans` | Orphans | `pacman -Qdtq`, after the upgrade. Asked about (default no); `--yes` only reports; `--remove-orphans` removes. |
| `services` | Services | `systemctl --failed` (system and user) before and after; units that newly failed are reported with `systemctl status`. |
| `setup` | Bindle setup | What's new ([UPGRADE-PATH.md](UPGRADE-PATH.md)): what this Bindle would add, change or retire compared with earlier applies, reviewed with `bindle whats-new` or the window's What's new button; after Bindle updated itself, offered right after the run. Also files Bindle wrote that changed by hand since (for modules whose catalog content didn't change), and drives in Bindle's fstab block that aren't connected. Reported only. |
| `reboot` | Reboot | Recommended when the running kernel's modules are gone (a kernel upgrade), or a kernel, microcode, NVIDIA driver, Mesa, systemd, glibc or D-Bus was upgraded. "Don't restart yet" when a DKMS module (`dkms status`) isn't built for an installed kernel, or a kernel's `vmlinuz-`/`initramfs-` image is missing from /boot. After an upgrade it fixes it without asking (the row then reads "builds the missing … ; then restart"): `dkms autoinstall -k <version>` for each kernel, then `mkinitcpio -P`, as root; until that succeeds the window shows no Restart button. `--check` reports it with the commands. /boot is often root-only: a run that started the root helper has it list /boot's file names; otherwise the images aren't checked, and it says so. On a system that boots unified kernel images (a UKI found on the EFI partition, or a mkinitcpio preset with a `_uki=` line) a missing `initramfs-*.img` in /boot is normal and not reported; (UKIs in `EFI/Linux` on the EFI partition) it reads the kernel version inside each one (its `.uname` section, straight from the file, no `objcopy`) and compares them with the installed kernels (those a package owns: the copy kernel-modules-hook keeps of the running kernel is not one). An image holding a kernel that is no longer installed (a leftover "previous" image, which can be first in the boot menu and start without its modules) is reported with the file's path and what to do; an installed kernel no image holds is "don't restart yet", fixed by `mkinitcpio -P` as above. The same rule as Bindle's boot-guard pacman hook. |

At a terminal `bindleup` starts with one list of what it would do (each item on; Enter starts,
numbers switch items off), plus the questions that default to "no" (Arch news asking for a manual
step, upgrading on battery). Then the password is asked once and the run goes to the end without
y/n questions, pacman prompts (pacman takes its default answers, as with `--yes`) or AUR
build-file review; anything that failed or was held is listed at the end. A failed snapshot holds
the upgrade rather than upgrading without one. `--yes` skips the list and takes the defaults.

The password goes in polkit's dialog (on uQ, the shell's), never in the terminal. While the dialog is
up the terminal doesn't echo, and anything typed into it then, or during the unattended run, is thrown
away when the helper answers, before pacman's own questions and when bindleup ends, so a password typed
there by mistake never reaches the shell (its prompt, or its history as a command).

Each stage prints `==> [n/N] Title: summary` and its details (up to 12
lines). The run ends with one row per stage: done, current, note, skipped,
held or failed. Exit status 1 when a stage failed. Each failed stage is also written to the user journal as `bindle: [BINDLE-004] bindleUP: <stage> failed: …` (`systemd-cat -t bindle`), from the terminal, the window and uQ Settings alike, which uQ's Settings › Problems reads (uQ 0.3.36+, through `identifiers` in `uq/diag/bindle.toml`).

## Preferences

`/etc/bindle/upgrade.toml` (the system's), then `~/.config/bindle/upgrade.toml`
(yours); each value comes from the last file that sets it. A broken file is
skipped with a warning. `bindleup --prefs` prints the values in effect
as a file; `bindo upgrade-prefs` writes yours from that and opens it.

```toml
mirror_max_age_days = 14   # refresh the mirror list when it is this old
news_days = 30             # news shown when pacman.log has no past upgrade
firmware_refresh_days = 7  # refresh fwupd's metadata when it is this old
battery_min_percent = 40   # on battery: below this, package upgrades wait
battery_max_packages = 30  # on battery: this many packages (or a kernel) wait too
bar_icon = "updates"       # uQ's bar: the bindleUP icon only when updates are ready, or "always"

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

## For uQ Settings: `bindle config` and `--ndjson`

uQ Settings' Bindle page ([GUI.md](GUI.md#uq-integration)) uses only the command line.

- `bindle config show [--json]` lists every preference with its value, default, range and where the
  value comes from (default, the system's file or yours). `config set <key> <value>` writes one
  key of your `upgrade.toml` after checking the whole file (`--dry-run` only checks; a broken
  file is never overwritten); `config reset <key>` removes it. Keys: `news_days`,
  `mirror_max_age_days`, `firmware_refresh_days`, `space.root_mb`, `space.boot_mb`,
  `space.efi_mb`, `battery_min_percent`, `battery_max_packages`, `releases.follow`,
  `releases.apps.<app>` and `update_notify` (`on`/`off`: the daily-check timer of `bindo
  update-notify`). Writing rewrites your file without its comments.
- `bindleup --check --ndjson` prints a `step` event for each thing it looks at, then one `plan`
  with every stage (`id`, `title`, `state`: run, note, current, skipped or held; `summary`, `why`,
  `details`, `needs_root`).
- `bindleup --ndjson [--only <stage>]` runs unattended, like `--yes`, the AUR and rebuilds
  included (as in the window): `stage` (index, total, id, title, summary), `line` (what a command
  said), `ask` (a question and the default it took), a `result` per stage, then `done` (`failed`).
  After Bindle replaced itself, `relaunch` is sent and the new one's events follow from the first
  stage. Exit status 1 when a stage failed. Unread Arch news that asks for a manual step still
  holds the package stages. uQ Settings stops a command after ten minutes, so a bigger update
  belongs in a terminal.
- `bindleup --dry-run --json` prints what would change as one plan document, `{summary, changes
  [{group, label, detail, reversible}], would_refuse, reason}`, the shape every
  `--dry-run --json` of the uQ apps shares; `undo --dry-run --json [<id>]` and
  `whats-new --dry-run --json` print the same for undoing and for applying What's new.

## Checks in the background

`bindo update-notify on` installs a user timer (`~/.config/systemd/user/bindle-update-check.timer`: daily and 15 minutes after login) that runs `bindleup --check --notify`. When there is work, a notification (through the desktop's notification server: on uQ, noti-fi) lists it with Update and Later; Update opens `bindleup --gui`, or bindleup in a terminal where the window can't start. `bindo update-notify off` removes the timer.

## In uQ's bar

On uQ, Bindle's package installs a bar widget (`uq/widgets/bindleup/`, to
`/usr/share/uq/shell/widgets/bindleup/`): an icon in the Status island with the
number of updates ready. Click it for the list, **Update…** (the update window,
`bindleup --gui`, or bindleup in a terminal where the window can't start) and
**Check now**; a middle click opens the window straight away. With
`bar_icon = "updates"` (the default) the icon shows only while something is
ready; with `"always"` it stays. Bindle's page in uQ Settings has the choice
under Preferences → uQ bar.

Every check (`bindleup --check` in any form, the daily timer, uQ Settings, the
window) saves what it found to `~/.local/state/bindle/updates.json`; a real run
removes it. `bindle updates [--json]` reads it, without the network. The icon
reads that every minute while the bar shows, and runs a check itself when the
last one is more than six hours old (at most every 30 minutes after one fails).
Only the stages that update something count: Bindle, packages, AUR, Bindle
apps, Flatpak, firmware and rebuilds, held ones included.

## The window

`bindleup --gui` opens the same run in Bindle's desktop look
(`bindle-gui --upgrade`, see [GUI.md](GUI.md#the-update-window)): a Check page
with a switch per stage, then the run (the AUR included) and a result table with
Restart and the log. `-n` and `--sandbox` work there too.

## Design

### Its own command, built like `bindle apply`

`bindleup` is a `bindle` subcommand with its own name, not a bindo
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
   helper (`RunAsRoot` steps and `InstallFiles`), started before the first
   stage when any stage needs root (or at the first root step otherwise): one
   password prompt for the whole run (pkexec, like every approval in Bindle),
   logged to `/var/log/bindle/upgrade-*.log`, and the helper's lock keeps it
   from running beside `bindle apply`. `pacman -Syu` goes to the helper as an
   `interactive` request naming your terminal (`/dev/pts/N`): the helper
   checks it is a terminal device owned by the user who authenticated
   (`PKEXEC_UID`) and runs pacman on it. Nothing else asks: Flatpak's
   system installation and fwupd run as root through the helper, and the AUR
   helper runs as you with `--sudo <shim>`: its `sudo pacman …` calls reach
   the helper through a socket only you can open (`bindle-run/src/bridge.rs`;
   a random token per run, `pacman` only, never with `--root`, `--dbpath`,
   `--hookdir`, `--config` and the like, `-U` only for package files under
   your home, `/var/cache` or `/tmp`), so builds and installs need no
   further password. The one exception: a run that replaces Bindle itself
   asks again in the new Bindle, which does the rest.

The look at the system (`probe::gather`) runs its network checks (Bindle's
release, news, pacman's databases, the AUR, Flatpak, fwupd) and the local
ones side by side, so start-up takes as long as the slowest one, not their
sum. AUR builds get `MAKEFLAGS=-jN` when neither your environment nor
`/etc/makepkg.conf` sets it.

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
- Bindle's self-update covers `/usr/local/bin` and AppImages, for systems without an AUR helper.
  With one, the `bindle` module (What's new offers it from 0.1.49) moves Bindle to its AUR
  package, which the AUR stage updates; `/usr/local/bin/bindle` then points at it and the
  self-update only reports.
- No `--json` for the closing table.
- No preferences page in the bindle-gui window: the file is the preference (`bindo upgrade-prefs`
  opens it, `bindle config` edits it, uQ Settings' Bindle page shows it).
