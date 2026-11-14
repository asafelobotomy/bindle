# Mounting other drives at startup

A fresh install mounts the system's own partitions and nothing else: a second
disk, a data partition or a Windows volume has to be added to `/etc/fstab` by
hand. Bindle offers each such drive in the wizard (Hardware → *Drives to mount
at startup*) and writes the fstab line for it.

```bash
bindle detect                       # "Other drives" lists what was found
bindle list                         # one mount-<uuid> module per drive
bindle plan --with mount-<uuid>     # the review prints the exact line
```

Nothing is pre-selected, in any profile: mounting a drive is always the
user's choice.

## What is offered

Detection needs no root: `lsblk --json` (the udev database) and `/etc/fstab`
(`crates/bindle-core/src/detect/drives.rs`, pure function `drives_from`).

| Found | Result |
|---|---|
| ext2/3/4, XFS, Btrfs, F2FS, NTFS, exFAT, FAT with a UUID, on any disk | offered |
| Mounted where the system uses it (`/`, `/boot`, `/home`, swap, the live ISO's stick: anywhere but `/mnt`, `/media`, `/run/media`) | left out |
| Another member of a system filesystem (multi-device Btrfs) | left out |
| EFI system, Microsoft reserved, Windows recovery, BIOS boot and swap partitions, by partition type | left out |
| LVM and RAID members, swap, ISO images, loop devices, optical drives, a filesystem without a UUID | left out (logical volumes on them are offered) |
| LUKS or BitLocker volume | listed as "Not available here": unlocking at startup needs a key and `/etc/crypttab`, which Bindle doesn't set up |
| Already in `/etc/fstab` outside Bindle's block (matched by UUID, device, label or PARTUUID) | listed as "Not available here: already in /etc/fstab, mounted at …" |
| Several devices with one UUID (multi-device Btrfs) | one entry |

"The system's own partitions" means the ones it uses, not everything on its
disk: a Windows or data partition beside the root partition is offered.

Each drive is one generated module (`builtins::drives::modules`, added by
`Catalog::with_drives`), so it is ticked, saved in `selection.toml` and
replayed like any catalog module. Its name says what it is
(`Mount Storage (932 GiB Btrfs)`), its description which device and where it
will mount, and its details the line it adds. A saved selection naming a drive
that is not connected is skipped with that reason, not refused.

## The line that is written

Always `UUID=`: device names (`/dev/sdb1`) change between boots.

| Filesystem | Type and options | Why |
|---|---|---|
| ext2/3/4 | `defaults`, pass `2` | checked by fsck after the root filesystem |
| XFS, F2FS | `defaults`, pass `0` | they check themselves on mount |
| Btrfs | `defaults,noatime,compress=zstd`, pass `0` | compresses what is written from now on; `noatime` stops reads rewriting metadata that snapshots share. Mounts the default subvolume (listing subvolumes would need root) |
| NTFS | `ntfs3`, `uid=,gid=,umask=022,windows_names` | the kernel's own driver, so no `ntfs-3g`; the volume has no Linux owners, so it belongs to the user running Bindle; `windows_names` refuses names Windows can't open |
| exFAT | `uid=,gid=,umask=022` | no owners on disk |
| FAT | `uid=,gid=,umask=022,utf8` | no owners on disk; non-ASCII names |
| every drive | `nofail,x-systemd.device-timeout=30s` | with `nofail` systemd neither requires the mount nor waits for it, so a missing or broken drive never stops the boot; the timeout (default 90 s) only bounds the background job and leaves a disk time to spin up |
| removable (USB, hot-plug) | plus `noauto,x-systemd.automount,x-systemd.idle-timeout=5min` | mounted when first opened and released when idle, so it can be unplugged. An fstab entry also means the file manager no longer mounts it under `/run/media` |

`umask=022` rather than a stricter `fmask`: files stay executable, or games
and scripts kept on a shared NTFS or exFAT drive would not run.

No packages are needed: every driver is in the kernel.

**Mount point**: `/mnt/<label>`, with anything but letters, digits, `-`, `_`
and `.` turned into `_`. No label: `/mnt/drive-<first 8 of the UUID>`. A name
that fstab or another drive already uses gets `-2`, `-3`…. A drive that is
already in Bindle's block keeps its mount point, even if its label changed.

## Where it is written

fstab has no drop-in directory. Bindle keeps its lines in one marked block at
the end of `/etc/fstab` and never edits a line outside it:

```
# BEGIN Managed by Bindle: drives mounted at startup
UUID=179dfc88-…	/mnt/Storage	btrfs	defaults,noatime,compress=zstd,nofail,x-systemd.device-timeout=30s	0 0
# END Managed by Bindle
```

The alternative was a `.mount` (and `.automount`) unit per drive in
`/etc/systemd/system`, which would be plain drop-in files. The block won
because fstab is what the user asked to stop editing and what everything else
reads: `mount /mnt/Storage`, `findmnt --fstab`, GNOME Disks and udisks (which
mounts a drive where fstab says) see the entry, a
later hand edit is where people expect it, and systemd generates the same units
from it anyway. Unit files would also need escaped names (`mnt-My\x2dDisk.mount`).
Like `pacman.conf`, it is a line-level edit by a pure, unit-tested function
(`crates/bindle-core/src/fstab.rs`).

- **Re-runnable**: a drive's line in the block is replaced, never added twice;
  a line that is already there exactly as planned is dropped from the plan.
- **Additive**: unticking a drive later does not remove its line. What's new
  ([UPGRADE-PATH.md](UPGRADE-PATH.md)) offers a clean-up for modules that were
  retired or taken out of the selection, but mount points are left out of the
  per-module record, so that clean-up never touches `/etc/fstab`. Delete the
  line from the block, or run `bindle undo`. bindleupgrade's "Bindle setup"
  stage reports drives in the block that aren't connected.
- **Undo**: `/etc/fstab` is snapshotted before the edit and restored byte for
  byte; the mount points Bindle created are removed once empty; systemd is
  reloaded. A drive that is mounted stays mounted until restart or `umount`,
  and undo says so. Restoring the file also drops any edit made to fstab
  after that apply.

The step (`Step::MountAtBoot`) runs in the root helper: write the block, create
the mount points, `systemctl daemon-reload`, then `mount` each drive that is
not mounted anywhere. A drive that fails to mount now is reported but does not
fail the apply: its line is `nofail` and is tried at every start. A drive that
is mounted elsewhere (by the file manager) moves to its new place at the next
start.

## Not covered

- **`bindle iso` / `bindle install`**: the live installer does not offer
  drives. Its detection describes the live environment and the disk about to
  be wiped; the first-login wizard on the installed system offers them instead.
- Encrypted volumes (`/etc/crypttab`), Btrfs subvolume choice, network shares.
- NTFS left "dirty" by Windows Fast Startup or hibernation will not mount
  until Windows shuts down fully; the module's note says so.
