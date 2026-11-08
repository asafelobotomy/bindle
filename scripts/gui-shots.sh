#!/usr/bin/env bash
# Render every wizard screen offscreen, once per uQ theme fixture and once with
# the desktop's own theme ("none"), into dist/shots/<theme>/.
set -euo pipefail
cd "$(dirname "$0")/.."
bin=target/debug/bindle-gui
for tokens in crates/bindle-gui/tests/fixtures/*.json none; do
    name=$(basename "$tokens" .json)
    mkdir -p "dist/shots/$name"
    BINDLE_UQ_TOKENS="$tokens" "$bin" --screenshots "dist/shots/$name"
done
find dist/shots -name '*.png' | sort
