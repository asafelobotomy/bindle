#!/bin/sh
# Managed by Bindle
# Bind Print to bindle-screenshot in a window manager's own config.
#   print-key <wm>            add Bindle's block (a re-run replaces it)
#   print-key <wm> --remove   take it out again; the rest stays as it was
# A missing config starts as the window manager's default. The block sits
# between "BEGIN Bindle" and "END Bindle" comment lines.
# Exit 3: Hyprland hasn't written its config yet (it does when it first
# starts): log in to Hyprland once, then run `bindo screenshot-key`.
set -eu
wm=${1:?usage: print-key <wm> [--remove]}
mode=${2:-add}
cfg=${XDG_CONFIG_HOME:-$HOME/.config}
cmd=/usr/local/bin/bindle-screenshot
root=${PRINT_KEY_ROOT:-} # tests: where the defaults are
begin="BEGIN Bindle: Print takes a screenshot of an area"
end="END Bindle"
close="" # what ends a comment line (XML's -->)
tmp=
trap 'rm -f "$tmp"' EXIT # a failed run leaves no half-written copy behind

# The file minus Bindle's block, with the lines Bindle switched off
# ("<c> Bindle off: …") switched back on.
without() {
    awk -v b="$c $begin" -v e="$c $end" -v off="$c Bindle off: " -v cl="$close" '
        { t = $0; sub(/^[ \t]+/, "", t) }
        index(t, b) == 1 { skip = 1; next }
        skip && index(t, e) == 1 { skip = 0; next }
        skip { next }
        index(t, off) == 1 {
            sub(/^[ \t]*/, "", $0); $0 = substr($0, length(off) + 1)
            if (cl != "") sub(" " cl "$", "")
        }
        { print }' "$file"
}

marker() { printf '%s %s%s\n' "$c" "$1" "${close:+ $close}"; }

# Write $tmp over $file, keeping its mode.
commit() {
    chmod --reference="$file" "$tmp" 2>/dev/null || true
    mv -f "$tmp" "$file"
}

append() {
    tmp=$(mktemp "$file.bindle.XXXXXX")
    { without; marker "$begin"; printf '%s\n' "$lines"; marker "$end"; } >"$tmp"
    commit
}

# The block right after the first line matching $open (niri's `binds {`);
# lines matching $off (another bare Print bind) are switched off.
inside() {
    tmp=$(mktemp "$file.bindle.XXXXXX")
    if ! without | awk -v open="$open" -v off="${off:-}" -v b="$(marker "$begin")" -v e="$(marker "$end")" \
        -v body="$lines" -v pre="$c Bindle off: " -v cl="${close:+ $close}" '
        done != 1 && $0 ~ open { print; print b; print body; print e; done = 1; next }
        off != "" && $0 ~ off { print pre $0 cl; next }
        { print }
        END { if (done != 1) exit 4 }' >"$tmp"; then
        rm -f "$tmp"
        echo "print-key: $file has no line matching $open to put the binding under" >&2
        exit 1
    fi
    commit
}

seed() {
    [ -e "$file" ] && return 0
    [ -f "$1" ] || { echo "print-key: no $file, and no default ($1) to start from" >&2; exit 1; }
    mkdir -p "$(dirname "$file")"
    cp "$1" "$file"
}

case "$wm" in
    sway) file=$cfg/sway/config c="#" default=$root/etc/sway/config
        lines="bindsym Print exec $cmd" ;;
    i3) file=$cfg/i3/config c="#" default=$root/etc/i3/config
        if [ ! -e "$file" ] && [ -f "$HOME/.i3/config" ]; then file=$HOME/.i3/config; fi
        lines="bindsym Print exec --no-startup-id $cmd" ;;
    awesome) file=$cfg/awesome/rc.lua c="--" default=$root/etc/xdg/awesome/rc.lua
        lines="root.keys(require(\"gears\").table.join(root.keys(), require(\"awful\").key({}, \"Print\", function() require(\"awful\").spawn(\"$cmd\") end, { description = \"screenshot of an area\", group = \"launcher\" })))" ;;
    qtile) file=$cfg/qtile/config.py c="#" default=$root/usr/share/doc/qtile/default_config.py
        lines="from libqtile.config import Key as _BindleKey
from libqtile.lazy import lazy as _bindle_lazy
keys.append(_BindleKey([], \"Print\", _bindle_lazy.spawn(\"$cmd\"), desc=\"Screenshot of an area\"))" ;;
    bspwm) file=$cfg/sxhkd/sxhkdrc c="#" default=$root/usr/share/doc/bspwm/examples/sxhkdrc
        lines="Print
	$cmd" ;;
    niri) file=$cfg/niri/config.kdl c="//" default=$root/usr/share/doc/niri/default-config.kdl
        lines="    Print { spawn \"$cmd\"; }" open='^binds[ \t]*[{]' off='^[ \t]*Print[ \t]*[{]' ;;
    labwc) file=$cfg/labwc/rc.xml c="<!--" close="-->" default=
        lines="<keybind key=\"Print\"><action name=\"Execute\" command=\"$cmd\" /></keybind>" open='<keyboard[ >]' ;;
    hyprland) file=$cfg/hypr/hyprland.lua c="--" default=
        lines="hl.bind(\"Print\", hl.dsp.exec_cmd(\"$cmd\"))"
        if [ ! -e "$file" ] && [ -e "$cfg/hypr/hyprland.conf" ]; then
            file=$cfg/hypr/hyprland.conf c="#" lines="bind = , Print, exec, $cmd"
        fi ;;
    *) echo "print-key: unknown window manager $wm" >&2; exit 2 ;;
esac

if [ "$mode" = --remove ]; then
    [ -f "$file" ] || exit 0
    tmp=$(mktemp "$file.bindle.XXXXXX")
    without >"$tmp"
    commit
    exit 0
fi

case "$wm" in
    hyprland)
        if [ ! -e "$file" ]; then
            # Hyprland writes its config when it first starts. Setting up from
            # the live ISO (a chroot): fail, so this runs again at first login.
            if systemd-detect-virt -q --chroot 2>/dev/null; then
                echo "print-key: Hyprland hasn't written its config yet; again at first login" >&2
                exit 1
            fi
            exit 3
        fi
        append ;;
    niri)
        seed "$default"
        inside ;;
    labwc)
        if [ ! -e "$file" ]; then
            mkdir -p "$(dirname "$file")"
            printf '<?xml version="1.0"?>\n<labwc_config>\n<keyboard>\n<default />\n</keyboard>\n</labwc_config>\n' >"$file"
        elif ! grep -q '<keyboard' "$file"; then
            # A keyboard section of Bindle's own, keeping labwc's default keys.
            tmp=$(mktemp "$file.bindle.XXXXXX")
            sed 's|</labwc_config>|<keyboard>\n<default />\n</keyboard>\n</labwc_config>|' "$file" >"$tmp"
            commit
        fi
        inside ;;
    bspwm)
        seed "$default"
        rc=$cfg/bspwm/bspwmrc
        if [ ! -e "$rc" ]; then
            mkdir -p "$(dirname "$rc")"
            cp "$root/usr/share/doc/bspwm/examples/bspwmrc" "$rc"
            chmod +x "$rc"
        fi
        append ;;
    *)
        seed "$default"
        append ;;
esac
