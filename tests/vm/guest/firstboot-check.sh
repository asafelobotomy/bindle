#!/bin/bash
# First boot of the installed system (test-only service, runs before GDM).
# Exercises Bindle as the test user, including a real apply.
echo "BINDLE-VM: CHECK START"
# Test-only: passwordless sudo for the test user (validated), so apply can run unattended.
echo 'tester ALL=(ALL:ALL) NOPASSWD: ALL' >/etc/sudoers.d/zz-bindle-vm
chmod 440 /etc/sudoers.d/zz-bindle-vm
visudo -cq || echo "BINDLE-VM: sudoers invalid"
grep -q '^@includedir /etc/sudoers.d' /etc/sudoers || echo '@includedir /etc/sudoers.d' >>/etc/sudoers
echo "BINDLE-VM: sudo rules for tester:"; sudo -l -U tester | tail -n 3
for _ in $(seq 60); do curl -fsS -o /dev/null --max-time 5 https://archlinux.org && break; sleep 3; done
as_tester() {
    echo "BINDLE-VM: \$ $*"
    runuser -l tester -c "$*"
    echo "BINDLE-VM: exit=$?"
}
# Use the host's latest build if it's being served (re-runs of the boot phase).
if curl -fsS --max-time 5 -o /tmp/bindle.new http://10.0.2.2:8765/bindle-x86_64 \
    && curl -fsS --max-time 5 -o /tmp/bindle.sha http://10.0.2.2:8765/bindle-x86_64.sha256 \
    && [[ "$(sha256sum </tmp/bindle.new | cut -d' ' -f1)" == "$(cut -d' ' -f1 /tmp/bindle.sha)" ]]; then
    install -m755 /tmp/bindle.new /usr/local/bin/bindle
    echo "BINDLE-VM: using the host's latest bindle build"
fi
# SECOND_LOGIN=1 on the host: pretend the wizard already opened once, so this
# graphical login should show the reminder notification instead.
if curl -fsS --max-time 5 -o /dev/null http://10.0.2.2:8765/second-login; then
    runuser -u tester -- sh -c 'mkdir -p ~/.local/state/bindle && touch ~/.local/state/bindle/first-login-seen'
    echo "BINDLE-VM: second-login mode (first-login-seen pre-set)"
fi
profile=$(sed -n 's/^profile = "\(.*\)"/\1/p' /etc/bindle/selection.toml)
as_tester "bindle archinstall"
as_tester "bindle first-login status"
as_tester "bindle plan --profile $profile"
# script(1) gives apply a terminal so sudo is used, as for a real user.
as_tester "script -qec 'bindle apply --profile $profile --yes' /dev/null"
as_tester "bindle undo --list"
echo "BINDLE-VM: second plan (should only contain non-idempotent leftovers):"
as_tester "bindle plan --profile $profile"
systemctl disable bindle-vm-check.service
echo "BINDLE-VM: CHECK DONE"
