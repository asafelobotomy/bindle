# Roadmap

## Phase 0: foundation ✅
- Rust workspace: `bindle-core` (catalog, detect, builtins, plan) + `bindle` binary
- Arch catalog (7 categories, 42 modules) and 4 profiles
- `bindle detect | list | plan [--json]` and a dry-run TUI wizard
- AppDir staging + `build-appimage.sh`

## Phase 0.5: archinstall-aware ✅
- Install record from archinstall's logged config, rebuilt from pacman.log when the log is missing ([ARCHINSTALL.md](ARCHINSTALL.md))
- Knowledge of all 21 archinstall desktops; `desktop-essentials` per desktop
- archinstall choices shown as ✓ and never redone; conflicting modules skipped; advisories
- Wizard opens on "Your install"; `bindle archinstall [--json]`

## Phase 1: apply ✅
- Root helper session (sudo / pkexec), AppImage-safe binary copy, version handshake ([APPLY.md](APPLY.md))
- Executor for every step with streamed output and logs in `/var/log/bindle/`
- Pre-apply snapshots, `bindle undo [--list]`
- AUR bootstrap (`makepkg` as the user → `pacman -U`) and AUR installs
- Already-enabled units and already-set pacman.conf options skipped
- Reboot/relogin summary; selection saved and replayable with `--from`
- Wizard: `a` applies from the Review screen
- First-login autostart: `bindle first-login install|remove|status|run|snippet`
- `--sandbox` mode: full pipeline without root

## Phase 1.2: live-ISO installer ✅
- `install.sh` (curl bootstrap) → `bindle iso`: pre-install screen, hardware-aware archinstall preset, archinstall plugin installing Bindle into the target ([ISO.md](ISO.md))
- First-login wizard seeded with the profile chosen before install
- CI and release workflows (static musl binary + checksum, AppImage, install.sh)

## Phase 1.5: hardening (next)
- Publish the GitHub repo and cut the first release (curl URL depends on it)
- ~~Full VM install from the current ISO~~ ✅ all 21 archinstall desktops ([VM-TEST.md](VM-TEST.md)); next: gaming/developer profiles, NVIDIA/AMD hardware passthrough
- Decide how first login coexists with desktop welcome apps (GNOME tour, KDE Welcome Center, Hyprland welcome)
- Sign releases (minisign) and verify in install.sh
- Real apply runs in an Arch VM per desktop (GNOME, Plasma, Hyprland at least)
- Turn advisories into optional steps (prune GPU drivers, iwd → NetworkManager)
- Progress view inside the TUI instead of dropping to the terminal

## Phase 2: distribution
- GitHub Actions in an `archlinux` container: lint, test, musl static build, AppImage, and the bare binary as release assets
- AppImage update info (zsync) and a short `curl … | sh`-free install doc for TTY users

## Phase 3: GUI
- Tauri 2 + React wizard over `bindle-core` (same plan/apply engine)

## Phase 4: catalog depth
- Desktop environment installs (KDE Plasma, GNOME, Hyprland) for minimal installs
- Bootloader integration: entries for extra kernels (systemd-boot, Limine, GRUB), snapper boot menus
- NVIDIA generation detection by PCI device id (legacy driver selection)
- Optional XZram install hand-off; zswap-vs-zram choice

## Phase 5: more distros
- Fedora, Debian/Ubuntu, openSUSE catalogs + package-manager backends
