# Recovery: Roll back, Rebase, Factory Restore

Three ways back when something isn't right, in the Recovery window
(`bindle recovery`, `bindo recovery`) and the terminal (`bindle rollback`,
`bindle rebase`, `bindle restore`). The window runs the same commands and
shows their output; what they do lives in `crates/bindle-core/src/recover/`
(planning, pure), `crates/bindle-run/src/helper/{recover,restore}.rs` (as root)
and `crates/bindle-cli/src/recover_cmd*` (asking and reporting).

| | Stays | Goes | Needs |
|---|---|---|---|
| Roll back | the home folder (`@home`) | system changes since the snapshot, kept aside | Btrfs root in a subvolume (`@`), snapper |
| Rebase | the home folder, packages the user added, Flatpaks, the machine's own settings | changes to the installed packages' config files (kept) | the installer's runs in `/var/log/pacman.log` |
| Factory Restore | nothing on the disk erased | everything | systemd-boot or GRUB, an unencrypted root, about 1.5 GB free |

## Roll back

`bindle rollback [N] [--list]`. The root helper gets only the snapshot number
(`Request::Rollback`); it reads the layout from the mount table itself
(`recover::rollback::layout`) and runs the script `recover::rollback::script`
makes:

1. Mount the filesystem's top (`subvolid=5`), check `<snapshots>/<N>/snapshot`.
2. Rename the root subvolume to `@.bindle-before-rollback-<time>` (never
   deleted here; `bindo rollback-cleanup` does that) and make a writable
   snapshot of N the new `@`. With snapper's directory inside `@`, it moves
   into the new one.
3. Take `subvolid=` off the root's fstab line (the new `@` has another id).
4. When /boot is its own partition (Bindle's default: the ESP), its kernels
   belong to no snapshot: the rolled-back system's own `vmlinuz` go there
   (`/usr/lib/modules/*/pkgbase`) and `mkinitcpio -P` runs in it, or the next
   start would boot a kernel whose modules it hasn't got.
5. A failure part way puts everything back: the old `@`, and /boot's files
   (copied first).

The record is `/var/lib/bindle/rollback.json`. `bindo rollback` and
`rollback-cleanup` come with the snapper module.

**From the boot menu.** With the installer kept on the computer (below), the
menu has "Bindle: Roll back the system": the Arch ISO starts with
`bindle.rollback=1` and runs `bindle rollback --installed-system`, which reads
the layout from the system's fstab (`layout_from_fstab`) and the snapshots
from snapper's `info.xml` files, then runs the same script (the ESP mounted
from fstab's UUID).

## Rebase

`bindle rebase [--undo-config ID]`:

- **Stock** is every package named in the installer's pacman runs
  (`pacman -r /mnt …` in pacman.log) plus every package Bindle's record lists.
- Reinstalls (`pacman -S`, not `--needed`) the stock packages that are
  installed, and puts back the ones that were removed (the review lists them:
  one removed on purpose may conflict with what replaced it).
- Resets the stock packages' changed config files (`pacman -Qkk`: size or
  checksum differs; a new modification time alone isn't a change) to the
  package's version from pacman's cache. Each changed copy is kept in
  `/var/lib/bindle/rebase/<id>/`; `--undo-config <id>` puts them back.
  `recover::rebase::KEEP` is never touched: accounts, fstab, crypttab,
  hostname, hosts, locale, vconsole, mkinitcpio, GRUB's defaults, the kernel
  command line, sudoers, the mirror list, machine-id, SSH and saved networks.
  The helper checks each file again (`ResetConfig`).
- A snapper snapshot first, where snapper is set up.
- Applies Bindle's saved selection again (`bindle apply --from … --yes`,
  which asks for the password a second time).

## Factory Restore

`bindle restore`, after typing ERASE (the window asks for it too):

1. The Arch ISO and its signature: downloaded from the Arch mirror when
   online, else the copy kept on the computer; neither: refused.
2. The root helper copies it to `/var/lib/bindle/restore/archlinux-x86_64.iso`
   and checks that copy (`gpgv` against pacman's Arch keyring, a revoked key
   refused): a user's file could change after a check, root's can't.
3. The ISO's kernel and initramfs, a copy of Bindle and `start.sh` go in
   `/boot/bindle-restore/`; a boot entry (`loader/entries/bindle-restore.conf`,
   or GRUB's `/etc/grub.d/42_bindle_restore`) starts the ISO with
   `img_dev=<root> img_loop=<the ISO> copytoram=y`,
   `systemd.mount-extra=<the /boot filesystem>:/run/bindle-restore`,
   `script=/run/bindle-restore/…/start.sh` and `bindle.restore=1`.
4. That entry is the next start's, once: `bootctl set-oneshot`, or for GRUB
   `grub-editenv set bindle_restore=1`, which the entry clears as it starts
   (grub-reboot needs `GRUB_DEFAULT=saved`, which archinstall doesn't set).
5. In the live system, `start.sh` copies Bindle into memory, lets go of the
   disk (the ISO is in memory: `copytoram`) and runs `bindle install`. With
   `bindle.restore=1` the disk the ISO came from isn't the boot medium to
   protect but the one to offer, pre-selected and marked "this computer's
   disk now" (`livesys::medium::restore_specs`). Nothing is erased until the
   wizard's last question; quitting restarts into the system as it was.

Refused: an encrypted root (the ISO's initramfs can't read a file inside
it: use a USB stick), Limine and rEFInd, and GRUB with /boot on Btrfs (GRUB
can't write its environment there, so "once" would be "always").
`bindle restore --cancel` takes the entry away.

**Keeping the installer** (`bindle restore --keep-image on|off`, the window's
switch): a checked copy stays on the computer, for Factory Restore without
the internet and, on Btrfs with snapper, the boot menu's "Roll back the
system" entry. Off deletes the copy, the staged files and that entry.

## Testing

Never on the host: `--sandbox <dir>` and `--dry-run` record or print the
commands. In the VM (`tests/vm/`, docs/VM-TEST.md) on a saved install.
