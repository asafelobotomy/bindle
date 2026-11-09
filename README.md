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
bash <(curl -fsSL https://raw.githubusercontent.com/asafelobotomy/bindle/main/install.sh)
```

Pick a desktop and profile, finish archinstall's pre-filled menu, reboot, and
Bindle greets you on first login. See [docs/ISO.md](docs/ISO.md).

> The repository is private until the first release, so this URL (and
> `releases/latest/download/…`) only works once it is public.

The AppImage, for an existing install, is on the
[releases page](https://github.com/asafelobotomy/bindle/releases/latest) as
`Bindle-x86_64.AppImage`.

## Status

**Status:** Phase 1. Detection, planning, apply (with snapshots and undo) and
first-login autostart work, verified by real installs and applies in VMs for
every archinstall desktop ([docs/VM-TEST.md](docs/VM-TEST.md)); not yet on
physical hardware. Arch Linux and derivatives only. See
[docs/ROADMAP.md](docs/ROADMAP.md).

```bash
cargo run -p bindle-cli               # wizard
cargo run -p bindle-cli -- archinstall  # what archinstall set up
cargo run -p bindle-cli -- detect     # what Bindle sees
cargo run -p bindle-cli -- list       # profiles, categories, modules
cargo run -p bindle-cli -- plan --profile gaming --with vscode --without ufw
cargo run -p bindle-cli -- apply --profile desktop     # asks, then snapshots + applies
cargo run -p bindle-cli -- undo --list
sudo ./target/debug/bindle first-login install         # open on each user's first login
```

Profiles: `minimal`, `desktop`, `gaming`, `developer`.
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
- [End-to-end VM test](docs/VM-TEST.md)
- [Catalog format](docs/CATALOG.md)
- [Roadmap](docs/ROADMAP.md)
