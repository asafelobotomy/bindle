#!/bin/sh
# Managed by Bindle
# Print: select an area of the screen, then mark it up in satty. Enter (or
# the save button) writes it to Pictures/Screenshots; the copy button puts it
# on the clipboard and saves it too. Esc while selecting takes nothing.
set -eu
pictures=$(xdg-user-dir PICTURES 2>/dev/null || true)
case "$pictures" in "" | "$HOME" | "$HOME/") pictures="$HOME/Pictures" ;; esac
dir="$pictures/Screenshots"
mkdir -p "$dir"
shot=$(mktemp --suffix .png)
trap 'rm -f "$shot"' EXIT
if [ -n "${WAYLAND_DISPLAY:-}" ]; then
    area=$(slurp) || exit 0
    grim -g "$area" "$shot"
    copy=wl-copy
else
    maim --select --hidecursor "$shot" || exit 0
    copy="xclip -selection clipboard -target image/png"
fi
satty --filename "$shot" --output-filename "$dir/Screenshot-%Y-%m-%d-%H%M%S.png" \
    --copy-command "$copy" --save-after-copy --actions-on-enter save-to-file --early-exit
