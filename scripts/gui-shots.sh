#!/usr/bin/env bash
# Render every wizard screen offscreen, once per uQ theme fixture and once with
# the desktop's own theme ("none"), into dist/shots/<theme>/. One more run,
# "applied", applies the plan in a sandbox (commands recorded, nothing run as
# root) and shots the finished Apply screen.
set -euo pipefail
cd "$(dirname "$0")/.."
bin=target/debug/bindle-gui
for tokens in crates/bindle-gui/tests/fixtures/*.json none; do
    name=$(basename "$tokens" .json)
    mkdir -p "dist/shots/$name"
    BINDLE_UQ_TOKENS="$tokens" "$bin" --screenshots "dist/shots/$name"
done
sandbox=$(mktemp -d)
trap 'rm -r -- "$sandbox"' EXIT
mkdir -p "$sandbox/etc" dist/shots/applied
cp /etc/pacman.conf "$sandbox/etc/"
BINDLE_UQ_TOKENS=crates/bindle-gui/tests/fixtures/rosewood-dark.json \
    "$bin" --screenshots dist/shots/applied --sandbox "$sandbox"
find dist/shots -name '*.png' | sort
