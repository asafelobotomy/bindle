"""Bindle plugin for archinstall.

Loaded by `bindle iso` via `archinstall --plugin`. When archinstall reaches
its on_install hook (base system installed, target mounted), this copies the
Bindle binary into the new system, registers it to open on first login, and
leaves the profile chosen before install for the first-login wizard. It
also saves the Wi-Fi joined on the ISO into the new system (NetworkManager).

Inputs come from the environment set by `bindle iso`:
  BINDLE_BIN        path to the bindle binary on the live ISO
  BINDLE_SELECTION  optional selection.toml chosen before install
  BINDLE_KEEP_WELCOME=1  leave the desktops' own welcome apps enabled
  BINDLE_SETUP_TARGET=1  apply the profile inside the new system before reboot
  BINDLE_USER       optional: the user to set up for (default: the first one)

Unless asked not to, it also turns off the desktops' welcome apps (GNOME tour,
KDE Welcome Center, Hyprland welcome) so Bindle is the first-login welcome:
once early (system defaults, picked up when the desktop's packages install)
and once at the end in on_genfstab, when users and Hyprland exist.

Nothing here may abort archinstall half-way through an install: every failure
is logged and skipped, and `bindle iso` reports a missing setup afterwards.
"""

import os
import shutil
import subprocess
from pathlib import Path

# archinstall compares this with its own major.minor and logs an error (but
# still loads the plugin) when this is lower. Bump after testing a release.
__archinstall__version__ = 4.5

try:  # archinstall's logger (also lands in /var/log/archinstall/install.log)
    from archinstall.lib.output import info as _info, warn as _warn
except Exception:  # pragma: no cover - older/newer archinstall layouts
    _info = _warn = None


def _log(msg: str, warning: bool = False) -> None:
    fn = _warn if warning else _info
    text = f'[bindle] {msg}'
    try:
        if fn is not None:
            fn(text)
            return
    except Exception:
        pass
    print(text, flush=True)


def _run(args: list, what: str) -> bool:
    """Run a command; log instead of raising when it fails."""
    try:
        result = subprocess.run(args, check=False)
    except OSError as e:
        _log(f'{what} could not start: {e}', warning=True)
        return False
    if result.returncode != 0:
        _log(f'{what} failed (exit {result.returncode}); continuing the install', warning=True)
        return False
    return True


# install-target's temporary passwordless sudo rule (bind-mounted over an
# empty placeholder while it runs). It never belongs in the installed system.
_GRANT = 'etc/sudoers.d/zz-bindle-install'


def _clear_grant(target: Path) -> None:
    """Remove any sudo grant install-target left behind (it removes its own)."""
    path = target / _GRANT
    if os.path.ismount(path):
        subprocess.run(['umount', str(path)], check=False)
        if os.path.ismount(path):
            subprocess.run(['umount', '-l', str(path)], check=False)
    try:
        path.unlink()
        _log(f'removed a leftover {path}', warning=True)
    except FileNotFoundError:
        pass
    except OSError as e:
        _log(f'could not remove {path}: {e}; delete it before restarting', warning=True)


class Plugin:
    def __repr__(self) -> str:
        return 'Bindle'

    def _quiet_welcome(self, target: Path) -> None:
        binary = os.environ.get('BINDLE_BIN')
        if os.environ.get('BINDLE_KEEP_WELCOME') == '1' or not binary or not Path(binary).is_file():
            return
        _run([binary, 'first-login', 'quiet-welcome', '--root', str(target)], 'turning off welcome apps')

    def on_genfstab(self, installation) -> None:
        # Runs last (after users and the desktop are installed): seed Hyprland
        # configs and make sure GNOME's schema override is compiled.
        target = Path(installation.target)
        self._quiet_welcome(target)
        if os.environ.get('BINDLE_KEEP_WELCOME') != '1' and (target / 'usr/bin/glib-compile-schemas').exists():
            _run(['arch-chroot', str(target), 'glib-compile-schemas', '/usr/share/glib-2.0/schemas'],
                 'compiling GSettings schemas')
        self._setup_target(target)
        return None  # let archinstall write fstab as usual

    def _setup_target(self, target: Path) -> None:
        # Apply the chosen profile in the new system now, while the ISO has a
        # network, so the first boot lands in a finished desktop. A failure
        # leaves Bindle to open at first login and finish, as before.
        if os.environ.get('BINDLE_SETUP_TARGET') != '1':
            return
        binary = os.environ.get('BINDLE_BIN')
        if not binary or not Path(binary).is_file() or not (target / 'usr/local/bin/bindle').is_file():
            return
        args = [binary, 'install-target', '--root', str(target)]
        user = os.environ.get('BINDLE_USER')
        if user:
            args += ['--user', user]
        try:
            _run(args, 'setting up the new system')
        finally:
            _clear_grant(target)

    def on_install(self, installation) -> None:
        target = Path(installation.target)
        binary = os.environ.get('BINDLE_BIN')
        if not binary or not Path(binary).is_file():
            _log('BINDLE_BIN not set; skipping first-login setup', warning=True)
            return

        # The Wi-Fi joined on the ISO (iwctl) becomes a NetworkManager
        # connection in the new system, so the first boot is online.
        _run([binary, 'carry-network', '--root', str(target)], 'saving the Wi-Fi for the new system')

        if not _run([binary, 'first-login', 'install', '--root', str(target)], 'installing Bindle for first login'):
            return
        self._quiet_welcome(target)

        try:
            etc = target / 'etc' / 'bindle'
            etc.mkdir(parents=True, exist_ok=True)
            selection = os.environ.get('BINDLE_SELECTION')
            if selection and Path(selection).is_file():
                shutil.copyfile(selection, etc / 'selection.toml')
                (etc / 'selection.toml').chmod(0o644)
            (etc / 'installed-by').write_text('bindle iso\n')
        except OSError as e:
            _log(f'could not save the chosen profile: {e}', warning=True)
            return
        _log('Bindle will open on first login')
