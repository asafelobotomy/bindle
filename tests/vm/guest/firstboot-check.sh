#!/bin/bash
# First boot of the installed system (test-only service, runs before GDM).
# Exercises Bindle as the test user, including a real apply.
echo "BINDLE-VM: CHECK START"
# Test-only: let the test user approve Bindle's actions without a password.
# Outside the installer's chroot Bindle asks through polkit (pkexec), never
# sudo, so a sudoers rule wouldn't help; this rule answers for Bindle only.
install -d -m 750 /etc/polkit-1/rules.d
cat >/etc/polkit-1/rules.d/49-bindle-vm-test.rules <<'RULES'
// Bindle VM test only: never on a real system.
polkit.addRule(function (action, subject) {
    if (subject.user !== "tester") return;
    var program = action.lookup("program") || "";
    if (action.id === "io.github.asafelobotomy.bindle.apply"
        || (action.id === "org.freedesktop.policykit.exec"
            && (program === "/usr/local/bin/bindle" || program.indexOf("/usr/lib/bindle/") === 0))) {
        return polkit.Result.YES;
    }
});
RULES
echo "BINDLE-VM: polkit rule for tester installed"
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
if curl -fsS --max-time 60 -o /tmp/bindle-gui.new http://10.0.2.2:8765/bindle-gui-x86_64 \
    && curl -fsS --max-time 5 -o /tmp/bindle-gui.sha http://10.0.2.2:8765/bindle-gui-x86_64.sha256 \
    && [[ "$(sha256sum </tmp/bindle-gui.new | cut -d' ' -f1)" == "$(cut -d' ' -f1 /tmp/bindle-gui.sha)" ]]; then
    install -m755 /tmp/bindle-gui.new /usr/local/bin/bindle-gui
    echo "BINDLE-VM: using the host's latest bindle-gui build"
fi
# SECOND_LOGIN=1 on the host: pretend the wizard already opened once, so this
# graphical login should show the reminder notification instead.
if curl -fs --max-time 5 -o /dev/null http://10.0.2.2:8765/second-login; then
    runuser -u tester -- sh -c 'mkdir -p ~/.local/state/bindle && touch ~/.local/state/bindle/first-login-seen'
    echo "BINDLE-VM: second-login mode (first-login-seen pre-set)"
fi
profile=$(sed -n 's/^profile = "\(.*\)"/\1/p' /etc/bindle/selection.toml)
# WITH="mod ..." on the host: extra modules for plan and apply (e.g. uQ's apps).
with=""
if curl -fs --max-time 5 -o /tmp/bindle-with http://10.0.2.2:8765/with; then
    for m in $(cat /tmp/bindle-with); do with="$with --with $m"; done
    echo "BINDLE-VM: extra modules:$with"
fi
as_tester "bindle archinstall"
as_tester "bindle first-login status"
as_tester "bindle plan --profile $profile$with"
# script(1) gives apply a terminal, as for a real user; pkexec is approved by
# the test rule above. With extra modules, --replace-conflicts does what the
# first-login wizard does (it always replaces).
as_tester "script -qec 'bindle apply --profile $profile$with --yes${with:+ --replace-conflicts}' /dev/null"
as_tester "bindle undo --list"
echo "BINDLE-VM: second plan (should only contain non-idempotent leftovers):"
as_tester "bindle plan --profile $profile$with"
systemctl disable bindle-vm-check.service
echo "BINDLE-VM: CHECK DONE"
