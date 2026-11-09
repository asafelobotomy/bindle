#!/bin/bash
# Runs on the Arch live ISO (via the `script=` boot parameter). Installs Arch
# through the real install.sh → bindle iso path, adds test-only hooks to the new
# system, and powers off. Progress goes to the serial console.
exec > >(tee -a /dev/ttyS0) 2>&1
base=http://10.0.2.2:8765
arg() { tr ' ' '\n' </proc/cmdline | sed -n "s/^$1=//p"; }

# Test-only: log the test user straight into the desktop so the first-login
# autostart can be screenshotted. One branch per login manager archinstall uses.
setup_autologin() {
    local key=${1%-dms} sessions session name exec
    key=${key%4}
    sessions=$(ls /mnt/usr/share/wayland-sessions/*.desktop /mnt/usr/share/xsessions/*.desktop 2>/dev/null)
    # shellcheck disable=SC2046 # one basename per word is the point
    echo "BINDLE-VM: sessions:" $(printf '%s\n' $sessions | xargs -rn1 basename)
    session=$(printf '%s\n' $sessions | grep -i -- "$key" | grep -m1 uwsm \
        || printf '%s\n' $sessions | grep -m1 "/$key.desktop$" \
        || printf '%s\n' $sessions | grep -im1 -- "$key" \
        || printf '%s\n' $sessions | head -1)
    name=$(basename "$session" .desktop)
    exec=$(sed -n 's/^Exec=//p' "$session" | head -1)
    echo "BINDLE-VM: autologin session=$name exec=$exec dm=$(readlink /mnt/etc/systemd/system/display-manager.service)"
    if [[ -d /mnt/etc/gdm ]]; then
        printf '[daemon]\nAutomaticLoginEnable=True\nAutomaticLogin=tester\n' >/mnt/etc/gdm/custom.conf
    fi
    if [[ -x /mnt/usr/bin/sddm ]]; then
        install -Dm644 /dev/stdin /mnt/etc/sddm.conf.d/zz-bindle-vm.conf <<<"$(printf '[Autologin]\nUser=tester\nSession=%s.desktop' "$name")"
    fi
    if [[ -e /mnt/usr/lib/pam.d/plasmalogin-autologin ]]; then
        for f in /mnt/etc/plasmalogin.conf.d/zz-bindle-vm.conf /mnt/etc/plasmalogin.conf; do
            install -Dm644 /dev/stdin "$f" <<<"$(printf '[Autologin]\nUser=tester\nSession=%s.desktop' "$name")"
        done
    fi
    if [[ -x /mnt/usr/bin/lightdm ]]; then
        install -Dm644 /dev/stdin /mnt/etc/lightdm/lightdm.conf.d/zz-bindle-vm.conf \
            <<<"$(printf '[Seat:*]\nautologin-user=tester\nautologin-session=%s' "$name")"
        arch-chroot /mnt sh -c 'groupadd -rf autologin && gpasswd -a tester autologin'
    fi
    for f in /mnt/etc/greetd/*.toml; do
        [[ -f "$f" ]] && printf '\n[initial_session]\ncommand = "%s"\nuser = "tester"\n' "$exec" >>"$f"
    done
    if [[ ! -e /mnt/etc/systemd/system/display-manager.service ]]; then
        # No greeter (e.g. Awesome): autologin on tty1 exercises the console hook.
        install -Dm644 /dev/stdin /mnt/etc/systemd/system/getty@tty1.service.d/autologin.conf <<'UNIT'
[Service]
ExecStart=
ExecStart=-/sbin/agetty -o '-p -f -- \\u' --noreset --noclear --autologin tester - ${TERM}
UNIT
    fi
}
desktop=$(arg bindle_desktop); profile=$(arg bindle_profile)
extra=(); [[ "$(arg bindle_keep_welcome)" == 1 ]] && extra+=(--keep-welcome)
echo "BINDLE-VM: guest start desktop=$desktop profile=$profile archinstall=$(archinstall --version 2>/dev/null)"
cd /tmp || exit 1
for f in archinstall-test.json creds-test.json firstboot-check.sh bindle-vm-check.service bindle-vm-diag.service; do
    curl -fsSO "$base/$f" || { echo "BINDLE-VM: INSTALL FAILED (fetch $f)"; poweroff; exit 1; }
done

# The host serves over plain http and signs with a throwaway key (run.sh).
if BINDLE_INSECURE=1 BINDLE_BASE_URL=$base bash <(curl -fsSL "$base/install.sh") \
    --desktop "$desktop" --profile "$profile" --yes --silent \
    --config /tmp/archinstall-test.json --creds /tmp/creds-test.json "${extra[@]}"; then
    # Test-only: run checks on first boot (it also sets passwordless sudo), GDM autologin.
    install -Dm755 firstboot-check.sh /mnt/usr/local/libexec/bindle-vm-check
    install -Dm644 bindle-vm-check.service /mnt/etc/systemd/system/bindle-vm-check.service
    install -Dm644 bindle-vm-diag.service /mnt/etc/systemd/system/bindle-vm-diag.service
    arch-chroot /mnt systemctl enable bindle-vm-check.service bindle-vm-diag.service
    setup_autologin "$desktop"
    echo "BINDLE-VM: target files:"
    ls -la /mnt/usr/local/bin/bindle /mnt/etc/xdg/autostart/bindle-first-login.desktop /mnt/etc/bindle/
    echo "BINDLE-VM: INSTALL OK"
else
    echo "BINDLE-VM: INSTALL FAILED"
    tail -n 80 /var/log/archinstall/install.log
fi
sync
poweroff
