#!/bin/sh
# Managed by Bindle
# Run by pacman after a kernel is installed, upgraded or removed. Unified kernel
# images (UKIs) on the boot partition must each hold a kernel that is still
# installed, and every installed kernel needs one. A leftover image (a "previous"
# copy, an old fallback) can sit first in the boot menu and start a kernel whose
# modules the upgrade just removed: then /boot, the firewall and drivers fail
# to load. Prints warnings only; it never stops a pacman run.
command -v objcopy >/dev/null 2>&1 || exit 0

# Kernels pacman owns (a copy kept by kernel-modules-hook for the running
# kernel is not one).
installed=
for d in /usr/lib/modules/*/; do
    v=${d%/}
    v=${v##*/}
    [ -f "${d}vmlinuz" ] || continue
    pacman -Qqo "${d}vmlinuz" >/dev/null 2>&1 && installed="$installed $v"
done

tmp=$(mktemp) || exit 0
trap 'rm -f "$tmp"' EXIT
seen=
found=0
unreadable=0
for dir in /boot/EFI/Linux /efi/EFI/Linux /boot/efi/EFI/Linux; do
    [ -d "$dir" ] || continue
    for f in "$dir"/*.efi; do
        [ -f "$f" ] || continue
        found=1
        # objcopy replaces its output file, so a pipe or /dev/stdout gives nothing.
        objcopy -O binary --only-section=.uname "$f" "$tmp" 2>/dev/null
        v=$(tr -d '\0\n' <"$tmp")
        # An image with no readable kernel version (built without .uname) is not provably stale
        # or missing a kernel: skip it, and don't claim an installed kernel has no image.
        [ -n "$v" ] || { unreadable=1; continue; }
        case " $installed " in
        *" $v "*) seen="$seen $v" ;;
        *)
            echo "bindle: boot-guard: $f holds kernel $v, which is not installed." >&2
            echo "bindle: boot-guard: it can be first in the boot menu and then boot without its modules." >&2
            echo "bindle: boot-guard: remove it, or move it out of $(dirname "$f"), then check the menu." >&2
            ;;
        esac
    done
done

# Only a system that boots from UKIs has anything to check.
[ "$found" = 1 ] || exit 0
[ "$unreadable" = 0 ] || exit 0
for v in $installed; do
    case " $seen " in
    *" $v "*) ;;
    *)
        echo "bindle: boot-guard: no boot image holds the installed kernel $v: run mkinitcpio -P." >&2
        ;;
    esac
done
exit 0
