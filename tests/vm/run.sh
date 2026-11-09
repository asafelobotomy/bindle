#!/usr/bin/env bash
# Real end-to-end install test in a QEMU/KVM VM, no clicks:
#   1. boot the official ISO headless; `script=` runs guest/guest-install.sh,
#      which uses the real install.sh → bindle iso → archinstall (unattended);
#   2. boot the installed disk; a test service runs `bindle apply` for real,
#      GDM auto-logs in and the first-login wizard should open; screenshots
#      are taken from the QEMU monitor.
#
#   tests/vm/run.sh [install|boot|all]     (default: all)
# Env: ISO, DESKTOP (gnome), PROFILE (desktop), MEM_INSTALL (2048), MEM_BOOT (2560), CPUS (2)
#      KEEP_WELCOME=1   install with the desktops' welcome apps left on (bindle iso --keep-welcome)
#      CLOSE_WELCOME=1  boot: press Alt+F4 after the third screenshot (closes a welcome app)
#      SECOND_LOGIN=1   boot: pre-mark the wizard as seen, so login shows the reminder notification
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
WORK="$HERE/.work"
ISO="${ISO:-$(ls -1 "$WORK"/archlinux*-x86_64.iso 2>/dev/null | tail -1)}"
DESKTOP="${DESKTOP:-gnome}"
PROFILE="${PROFILE:-desktop}"
MEM_INSTALL="${MEM_INSTALL:-2048}"
MEM_BOOT="${MEM_BOOT:-2560}"
CPUS="${CPUS:-2}"
PORT=8765
OVMF=/usr/share/edk2-ovmf/x64
PHASE="${1:-all}"

say() { printf '\033[1;36m[vm]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[vm] %s\033[0m\n' "$*" >&2; exit 1; }
[[ -f "$ISO" ]] || die "no ISO; download one into $WORK (see docs/VM-TEST.md)"
[[ -w /dev/kvm ]] || die "/dev/kvm not writable"

PIDS=()
cleanup() { ((${#PIDS[@]})) && kill "${PIDS[@]}" 2>/dev/null; true; }
trap cleanup EXIT

# Host-side helpers: guest files (8765) and a caching pacman mirror (8766).
# A server left over from an earlier run would serve stale files or old proxy
# code, so both ports must be free.
SERVERS_UP=0
start_servers() {
    local p
    # `all` runs both phases in one process: keep the servers the install started.
    ((SERVERS_UP)) && return 0
    for p in "$PORT" 8766; do
        if (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then
            die "port $p is already in use (a server from an earlier run?); stop it first"
        fi
    done
    python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$WORK/serve" >"$WORK/http.log" 2>&1 &
    PIDS+=($!)
    python3 -I "$HERE/pkgproxy.py" "$WORK/pkgcache" 8766 >>"$WORK/pkgproxy.log" 2>&1 &
    PIDS+=($!)
    SERVERS_UP=1
}

monitor() { socat - "UNIX-CONNECT:$WORK/mon.sock" <<<"$1" >/dev/null 2>&1 || true; }
# shellcheck disable=SC2054 # the commas are inside QEMU option values
qemu_base=(qemu-system-x86_64 -enable-kvm -cpu host -smp "$CPUS"
    -drive "if=pflash,format=raw,readonly=on,file=$OVMF/OVMF_CODE.4m.fd"
    -drive "if=pflash,format=raw,file=$WORK/vars.fd"
    -drive "file=$WORK/disk.raw,format=raw,if=virtio"
    -nic user,model=virtio-net-pci
    -monitor "unix:$WORK/mon.sock,server,nowait")

wait_for() { # file pattern timeout-seconds pid
    local deadline=$((SECONDS + $3))
    while ((SECONDS < deadline)); do
        grep -q "$2" "$1" 2>/dev/null && return 0
        kill -0 "$4" 2>/dev/null || return 1
        sleep 5
    done
    return 1
}

install_phase() {
    say "building bindle"
    rm -rf "$WORK/serve"
    serve_build
    cp "$HERE"/guest/* "$WORK/serve/"
    sign_test_release

    say "extracting kernel from $(basename "$ISO")"
    # ISO contents extract read-only.
    [[ -d "$WORK/iso" ]] && chmod -R u+w "$WORK/iso"
    rm -rf "$WORK/iso" && mkdir -p "$WORK/iso"
    bsdtar -xf "$ISO" -C "$WORK/iso" arch/boot/x86_64/vmlinuz-linux arch/boot/x86_64/initramfs-linux.img loader/entries
    chmod -R u+w "$WORK/iso"
    local opts
    opts="$(sed -n 's/^options *//p' "$WORK"/iso/loader/entries/01-archiso-linux.conf)"

    rm -f "$WORK/disk.raw" && truncate -s 24G "$WORK/disk.raw"
    cp "$OVMF/OVMF_VARS.4m.fd" "$WORK/vars.fd"
    : >"$WORK/install-serial.log"

    start_servers

    say "installing ($DESKTOP / $PROFILE, ${MEM_INSTALL} MiB); log: $WORK/install-serial.log"
    # The ISO as a second virtio disk (vdb): archiso's initramfs finds it by
    # uuid reliably there; the target stays /dev/vda.
    "${qemu_base[@]}" -m "$MEM_INSTALL" -drive "file=$ISO,format=raw,if=virtio,readonly=on" \
        -kernel "$WORK/iso/arch/boot/x86_64/vmlinuz-linux" \
        -initrd "$WORK/iso/arch/boot/x86_64/initramfs-linux.img" \
        -append "$opts copytoram=n console=tty0 console=ttyS0,115200 script=http://10.0.2.2:$PORT/guest-install.sh bindle_desktop=$DESKTOP bindle_profile=$PROFILE bindle_keep_welcome=${KEEP_WELCOME:-0}" \
        -serial "file:$WORK/install-serial.log" -display none -no-reboot &
    local qemu=$!
    if ! wait_for "$WORK/install-serial.log" "BINDLE-VM: INSTALL \(OK\|FAILED\)" 5400 "$qemu"; then
        kill "$qemu" 2>/dev/null || true
        die "install did not finish (see install-serial.log)"
    fi
    wait "$qemu" || true
    grep -q "BINDLE-VM: INSTALL OK" "$WORK/install-serial.log" || die "install failed (see install-serial.log)"
    # Keep the fresh install so `boot` can be re-run without reinstalling.
    cp --reflink=auto --sparse=always "$WORK/disk.raw" "$WORK/disk.installed.raw"
    say "install OK (saved as disk.installed.raw)"
}

shot() {
    monitor "screendump $WORK/shots/$1.ppm"
    sleep 2
    if [[ -f "$WORK/shots/$1.ppm" ]]; then
        python3 -I "$HERE/ppm2png.py" "$WORK/shots/$1.ppm" "$WORK/shots/$1.png" && rm -f "$WORK/shots/$1.ppm"
    else
        say "no screenshot $1 (VM not running?)"
    fi
}

serve_build() {
    (cd "$ROOT" && cargo build -q -p bindle-cli -p bindle-gui)
    mkdir -p "$WORK/serve"
    cp "$ROOT/target/debug/bindle" "$WORK/serve/bindle-x86_64"
    cp "$ROOT/target/debug/bindle-gui" "$WORK/serve/bindle-gui-x86_64"
    (cd "$WORK/serve" && sha256sum bindle-x86_64 >bindle-x86_64.sha256 \
        && sha256sum bindle-gui-x86_64 >bindle-gui-x86_64.sha256)
}

# install.sh only runs a binary whose SHA256SUMS is signed by its embedded
# key. The VM serves a local build, so sign it with a throwaway key and serve
# an install.sh that embeds that key instead (test copy only).
sign_test_release() {
    local gh="$WORK/test-gnupg" fpr
    rm -rf "$gh" && mkdir -m 700 "$gh"
    GNUPGHOME="$gh" gpg --batch -q --pinentry-mode loopback --passphrase '' \
        --quick-gen-key "Bindle VM test (not a release key)" ed25519 sign 1d
    fpr="$(GNUPGHOME="$gh" gpg --list-keys --with-colons | awk -F: '/^fpr/ { print $10; exit }')"
    python3 -I - "$ROOT/install.sh" "$WORK/serve/install.sh" "$fpr" "$(GNUPGHOME="$gh" gpg --armor --export "$fpr")" <<'PY'
import re, sys
src, dst, fpr, key = sys.argv[1:]
s = open(src).read()
s = re.sub(r"^SIGNING_FPR=.*$", f"SIGNING_FPR={fpr}", s, count=1, flags=re.M)
s = re.sub(r"^SIGNING_KEY='[^']*'", lambda _: f"SIGNING_KEY='{key}'", s, count=1, flags=re.M)
open(dst, "w").write(s)
PY
    (cd "$WORK/serve" && sha256sum bindle-x86_64 bindle-gui-x86_64 install.sh >SHA256SUMS \
        && GNUPGHOME="$gh" gpg --batch -q --yes --local-user "$fpr" --output SHA256SUMS.sig --detach-sign SHA256SUMS)
}

boot_phase() {
    [[ -f "$WORK/disk.installed.raw" ]] || die "run the install phase first"
    say "restoring the freshly installed disk"
    cp --reflink=auto --sparse=always "$WORK/disk.installed.raw" "$WORK/disk.raw"
    # The guest's check pulls this build, so code changes don't need a reinstall.
    serve_build
    rm -f "$WORK/serve/second-login"
    [[ "${SECOND_LOGIN:-0}" == 1 ]] && touch "$WORK/serve/second-login"
    start_servers
    mkdir -p "$WORK/shots" && rm -f "$WORK"/shots/*
    : >"$WORK/boot-serial.log"
    say "booting installed system (${MEM_BOOT} MiB); log: $WORK/boot-serial.log"
    "${qemu_base[@]}" -m "$MEM_BOOT" -vga std -display none \
        -serial "file:$WORK/boot-serial.log" &
    local qemu=$!
    if ! wait_for "$WORK/boot-serial.log" "BINDLE-VM: CHECK \(DONE\|FAILED\)" 3600 "$qemu"; then
        shot timeout
        kill "$qemu" 2>/dev/null || true
        die "first-boot checks did not finish (see boot-serial.log, shots/timeout.png)"
    fi
    if grep -q "BINDLE-VM: CHECK FAILED" "$WORK/boot-serial.log"; then
        kill "$qemu" 2>/dev/null || true
        die "first-boot checks failed (see boot-serial.log)"
    fi
    say "checks done; waiting for autologin and the first-login wizard"
    shot checks-done
    for i in 1 2 3 4 5 6; do
        sleep 30
        shot "desktop-$i"
        if [[ $i == 3 && "${CLOSE_WELCOME:-0}" == 1 ]]; then
            say "closing the welcome app (Alt+F4)"
            monitor "sendkey alt-f4"
        fi
    done
    monitor system_powerdown
    sleep 30
    kill "$qemu" 2>/dev/null || true
    say "screenshots: $WORK/shots/"
}

case "$PHASE" in
    install) install_phase ;;
    boot) boot_phase ;;
    all) install_phase && boot_phase ;;
    *) die "usage: $0 [install|boot|all]" ;;
esac
