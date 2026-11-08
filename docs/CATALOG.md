# Catalog format

One TOML file per category under `catalog/<distro>/`, registered in
`crates/bindle-core/src/catalog.rs` (`ARCH_FILES`). Profiles live in
`catalog/profiles.toml`. The catalog is validated at load time; `cargo test` fails
on a broken reference.

```toml
[category]
id = "performance"
name = "Performance"
description = "Memory, I/O and build tuning."
order = 30                      # wizard ordering

[[module]]
id = "io-schedulers"            # unique across the catalog
name = "I/O schedulers"
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
builtin = "zram"                # hardware-computed steps (gpu, microcode, zram, aur-helper, desktop)
notes = ["Shown on the review screen."]

[[module.files]]
path = "/etc/udev/rules.d/60-bindle-ioschedulers.rules"
mode = 0o644
content = '''
# Managed by Bindle
…
'''
```

## Command guards

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

`cargo test` validates the catalog: references, symmetric `conflicts`, `when`
tags (below; `desktop:<id>` must name a desktop in `desktops.toml`), absolute
file and check paths.

## `when` tags

`distro:<id>`, `cpu:<intel|amd|other>`, `gpu:<nvidia|amd|intel|virtual>`,
`fs:<root fs>`, `boot:<systemd-boot|grub|limine|refind|unknown>`,
`session:<wayland|x11|tty>`, plus flags `ssd`, `laptop`, `bluetooth`, `virt`,
`multilib`, `zram-active`, `zswap`, `hibernate`, `aur-helper`, `desktop`, `nm`,
`kdeconnect` (installed, or in a detected desktop's essentials),
`snapshots-mount` (`/.snapshots` is its own mount, e.g. archinstall's
`@.snapshots`), `hybrid-gpu` (NVIDIA plus Intel/AMD),
plus `desktop:<id>`, `toolkit:<qt|gtk|other>`, `display:<wayland|xorg>` and
`installer:<archinstall|pacstrap>`. Prefix with `!` to negate.

Modules the installer already set up (see [ARCHINSTALL.md](ARCHINSTALL.md)) are
reported as provided and never planned.

## Desktops

`catalog/desktops.toml` describes each archinstall desktop: how to recognise it
(`detect`, `archinstall` package set), its greeter, and the `essentials`/`nm`/`bt`
packages the `desktop` builtin adds. Check package names with `make check-packages`
(it also covers greeters, archinstall's lists, and the `PACKAGES` consts in
`crates/bindle-core/src/builtins/`, which unit tests keep in sync with builtin output).
Run `bindle detect` to see the facts for the current machine.

## Arch modules (v0.1)

| Category | Modules |
|---|---|
| System basics | pacman-qol, multilib, mirrors, build-tools, aur-helper, microcode, fstrim, firmware-updates |
| Hardware | gpu-driver, audio-pipewire, bluetooth, printing, power-profiles, tlp, tlp-rdw |
| Performance | zram, io-schedulers, oomd, makepkg-parallel, makepkg-native, kernel-zen |
| Desktop | fonts, codecs, flatpak, archives, user-dirs, firefox, desktop-essentials |
| Development | rustup, nodejs, python, docker, podman, neovim, vscode |
| Gaming | steam, gamemode, mangohud, wine, lutris, heroic |
| Security | ufw, firewalld, snapper |
