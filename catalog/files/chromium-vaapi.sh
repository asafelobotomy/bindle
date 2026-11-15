#!/bin/sh
# Managed by Bindle
# Hardware video decoding in Chromium-based browsers on Intel and AMD GPUs:
# AcceleratedVideoDecodeLinuxGL in each installed browser's flags file. A
# flags file with its own --enable-features gets the feature added to it
# (a second --enable-features line would replace the first).
set -eu
cfg=${XDG_CONFIG_HOME:-$HOME/.config}
feature=AcceleratedVideoDecodeLinuxGL

# Intel (0x8086) or AMD (0x1002) graphics: NVIDIA's VA-API path differs.
gpu=no
for v in /sys/class/drm/card*/device/vendor; do
    [ -f "$v" ] || continue
    case "$(cat "$v")" in 0x8086 | 0x1002) gpu=yes ;; esac
done
[ "$gpu" = yes ] || exit 0

for pair in chromium:chromium-flags.conf brave:brave-flags.conf google-chrome-stable:chrome-flags.conf vivaldi-stable:vivaldi-stable.conf; do
    bin=${pair%%:*}
    file=$cfg/${pair#*:}
    command -v "$bin" >/dev/null 2>&1 || continue
    if [ -f "$file" ] && grep -q "$feature" "$file"; then
        continue
    fi
    mkdir -p "$cfg"
    if [ -f "$file" ] && grep -q -- '--enable-features=' "$file"; then
        sed -i "s/--enable-features=/--enable-features=$feature,/" "$file"
    else
        printf -- '--enable-features=%s\n' "$feature" >>"$file"
    fi
    echo "$bin: hardware video decoding on ($file)"
done
