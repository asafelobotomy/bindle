#!/usr/bin/env bash
# Render branding/bindle.svg to the PNG sizes desktops look for (hicolor),
# and record which SVG they came from. Run after editing the SVG; the unit
# test `icons_match_the_svg` fails until you do.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v rsvg-convert >/dev/null || { echo "needs rsvg-convert (librsvg)" >&2; exit 1; }
out=branding/png
mkdir -p "$out"
for n in 16 22 24 32 48 64 128 256 512; do
    rsvg-convert -w "$n" -h "$n" branding/bindle.svg -o "$out/bindle-$n.png"
    if command -v oxipng >/dev/null; then oxipng -q -o max --strip safe "$out/bindle-$n.png"; fi
done
sha256sum branding/bindle.svg | cut -d' ' -f1 >"$out/SOURCE.sha256"
ls -l "$out"
