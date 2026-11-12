"""Open archinstall's own disk editor and save what was chosen.

Run by `bindle install` for "Manual partitioning" on its Disk page: this one
step uses archinstall's menu (partitioning, LVM, encryption, Btrfs
snapshots), everything else stays in Bindle's wizard. Nothing is written to
any disk here; the layout is only saved, in the format archinstall itself
saves and reads back (`disk_config` plus the passphrase it keeps apart), for
the unattended install that follows.

  BINDLE_DISK_OUT  file to write the result to (JSON, mode 0600)

Exit 0 without writing the file: the menu was left without a layout.
Exit 3: this archinstall has no disk editor Bindle knows how to open.
"""

import json
import os
import sys


def main() -> int:
    out = os.environ.get('BINDLE_DISK_OUT')
    if not out:
        print('BINDLE_DISK_OUT is not set', file=sys.stderr)
        return 2
    try:
        from archinstall.lib.disk.disk_menu import DiskLayoutConfigurationMenu
        from archinstall.tui.components import tui
    except Exception as e:  # another archinstall layout
        print(f"archinstall's disk editor isn't where Bindle expects it: {e}", file=sys.stderr)
        return 3

    # tui.run exits 0 by itself when the menu returns nothing.
    layout = tui.run(lambda: DiskLayoutConfigurationMenu(None).show())
    if layout is None:
        return 0

    result = {'disk_config': layout.json()}
    encryption = getattr(layout, 'disk_encryption', None)
    password = getattr(encryption, 'encryption_password', None)
    if password is not None and getattr(password, 'plaintext', ''):
        result['encryption_password'] = password.plaintext

    fd = os.open(out, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, 'w') as f:
        json.dump(result, f)
    return 0


if __name__ == '__main__':
    sys.exit(main())
