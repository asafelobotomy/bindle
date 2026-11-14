<img src="branding/bindle.svg" alt="" width="112" align="right">

# Bindle

Pack your fresh Linux install for the road. Bindle is a wizard you run right
after an **archinstall** desktop install: it opens on what archinstall set up
(desktop, greeter, drivers, audio, firewall…), detects your hardware, lets you pick a profile and
categories of packages, drivers, tools and optimisations, shows exactly what will
change, then applies it.

## Install Arch with Bindle

From the Arch Linux live ISO (online):

```bash
curl -fsSLO https://github.com/asafelobotomy/bindle/releases/latest/download/install.sh
bash install.sh
```

That `install.sh` is the release's own copy, listed in its signed `SHA256SUMS`
([how to check it](docs/ISO.md#trust)).

One wizard asks for the network, desktop, disk and your user; Bindle then installs
Arch, the desktop and your apps before the first reboot. See [docs/ISO.md](docs/ISO.md).

The AppImage, for an existing install, is on the
[releases page](https://github.com/asafelobotomy/bindle/releases/latest) as
`Bindle-x86_64.AppImage`.

## Status

**Status:** 0.1.8, hardening (Phase 1.5). The live-ISO installer, detection,
planning, apply (with snapshots and undo), first-login autostart, the desktop
window, bindo, bindleupgrade and What's new work. Installs and applies are
verified in VMs for every archinstall desktop through `bindle iso`
([docs/VM-TEST.md](docs/VM-TEST.md)); not yet on physical hardware. Arch Linux
and derivatives only. See [docs/ROADMAP.md](docs/ROADMAP.md).

```bash
cargo run -p bindle-cli               # wizard
cargo run -p bindle-cli -- archinstall  # what archinstall set up
cargo run -p bindle-cli -- detect     # what Bindle sees
cargo run -p bindle-cli -- list       # profiles, categories, modules
cargo run -p bindle-cli -- plan --profile gaming --with vscode --without ufw
cargo run -p bindle-cli -- apply --profile desktop     # asks, then snapshots + applies
cargo run -p bindle-cli -- undo --list
cargo run -p bindle-cli -- status     # what Bindle last set up here, and what is missing
cargo run -p bindle-cli -- whats-new --list   # what this Bindle adds or changes since the last apply
cargo run -p bindle-cli -- bindo      # recipes: named system tasks (also `bindo`)
cargo run -p bindle-cli -- upgrade --check    # update everything (also `bindleupgrade`)
cargo run -p bindle-cli -- install --dry-run  # the live-ISO installer (on the ISO: `bindle install`)
run0 ./target/debug/bindle first-login install         # open on each user's first login
make gui                                               # bindle-gui, the desktop window (needs Qt 6 and Kirigami)
```

Profiles: `minimal`, `desktop`, and the add-ons `gaming` and `developer`.
Categories: System · Hardware · Performance · Desktop & apps · Internet ·
Creative · Development · AI tools · Gaming · Security
(every module: [docs/CATALOG.md](docs/CATALOG.md#arch-modules)).

## AppImage

```bash
make appimage      # → dist/Bindle-<version>-x86_64.AppImage
```

The AppImage needs `fusermount3` (`pacman -S fuse3`). On a minimal install
without it, run `./Bindle-*.AppImage --appimage-extract-and-run`.

## Docs

- [Design and investigation](docs/DESIGN.md)
- [Installing from the live ISO](docs/ISO.md)
- [Building on archinstall](docs/ARCHINSTALL.md) (incl. first-login autostart)
- [Applying changes, snapshots and undo](docs/APPLY.md)
- [The desktop window](docs/GUI.md)
- [bindo: recipes](docs/BINDO.md)
- [bindleupgrade: updating everything](docs/UPGRADE.md)
- [What's new after upgrading Bindle](docs/UPGRADE-PATH.md)
- [End-to-end VM test](docs/VM-TEST.md)
- [Catalog format](docs/CATALOG.md)
- [Mounting other drives](docs/DRIVES.md)
- [Roadmap](docs/ROADMAP.md)
