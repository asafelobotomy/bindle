#!/usr/bin/env bash
# Run the end-to-end VM test for every archinstall desktop, one after another.
#   tests/vm/matrix.sh                 # all 21
#   DESKTOPS="plasma sway" tests/vm/matrix.sh
# Results: .work/results/<desktop>/ (serial logs, screenshots) and summary.tsv.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$HERE/.work"
RES="$WORK/results"
DESKTOPS="${DESKTOPS:-plasma hyprland xfce4 cinnamon cosmic sway niri mate budgie lxqt deepin enlightenment labwc river i3 awesome bspwm qtile xmonad niri-dms gnome}"
mkdir -p "$RES"
[[ -f "$RES/summary.tsv" ]] || printf 'desktop\tinstall\tapply\tleftover\tminutes\n' >"$RES/summary.tsv"

for d in $DESKTOPS; do
    start=$SECONDS
    : >"$WORK/install-serial.log"
    : >"$WORK/boot-serial.log"
    rm -f "$WORK"/shots/*
    echo "== $d: starting $(date +%H:%M)"
    DESKTOP="$d" "$HERE/run.sh" all >"$RES/$d.runner.log" 2>&1
    out="$RES/$d"
    rm -rf "$out" && mkdir -p "$out"
    cp "$WORK/install-serial.log" "$WORK/boot-serial.log" "$out/" 2>/dev/null
    cp -r "$WORK/shots" "$out/" 2>/dev/null
    install=FAIL apply=-
    grep -aq "BINDLE-VM: INSTALL OK" "$WORK/install-serial.log" && install=OK
    if grep -aqE "All [0-9]+ steps applied" "$WORK/boot-serial.log"; then
        apply=OK
    elif grep -aqE "Stopped at step|Error:" "$WORK/boot-serial.log"; then
        apply=FAIL
    fi
    leftover=$(tr -d '\r' <"$WORK/boot-serial.log" | sed -n '/second plan/,/CHECK DONE/p' | grep -cE '^ ?[0-9]+\. \[' || true)
    printf '%s\t%s\t%s\t%s\t%s\n' "$d" "$install" "$apply" "$leftover" $(((SECONDS - start) / 60)) >>"$RES/summary.tsv"
    echo "== $d: install=$install apply=$apply leftover=$leftover ($(((SECONDS - start) / 60)) min)"
done
column -t "$RES/summary.tsv"
