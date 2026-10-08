#!/usr/bin/env bash
# Bindle: install Arch Linux from the live ISO with Bindle's presets, then
# finish setting up on first login.
#
#   bash <(curl -fsSL https://raw.githubusercontent.com/asafelobotomy/bindle/main/install.sh)
#   curl -fsSL https://raw.githubusercontent.com/asafelobotomy/bindle/main/install.sh | bash
#
# Arguments are passed to `bindle iso` (e.g. --desktop plasma --profile gaming).
# Environment:
#   BINDLE_VERSION   release tag to fetch (default: latest)
#   BINDLE_REPO      GitHub owner/repo (default: asafelobotomy/bindle)
#   BINDLE_BASE_URL  override the download location entirely (testing/mirrors)
#   BINDLE_FORCE=1   skip the live-ISO check (only with --dry-run!)
set -euo pipefail

main() {
    local repo="${BINDLE_REPO:-asafelobotomy/bindle}"
    local version="${BINDLE_VERSION:-latest}"
    local base
    if [[ -n "${BINDLE_BASE_URL:-}" ]]; then
        base="$BINDLE_BASE_URL"
    elif [[ "$version" == latest ]]; then
        base="https://github.com/$repo/releases/latest/download"
    else
        base="https://github.com/$repo/releases/download/$version"
    fi

    say() { printf '\033[1;33m::\033[0m %s\n' "$*"; }
    die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

    [[ "$(uname -m)" == x86_64 ]] || die "Bindle supports x86_64 only."
    if [[ ! -d /run/archiso && "${BINDLE_FORCE:-0}" != 1 ]]; then
        die "This installs Arch Linux and wipes disks. Run it from the Arch Linux live ISO.
       On an installed system, download the Bindle AppImage instead."
    fi
    [[ "${BINDLE_FORCE:-0}" == 1 || "$(id -u)" -eq 0 ]] || die "Run as root (the live ISO shell already is)."
    command -v curl >/dev/null || die "curl is missing."
    command -v sha256sum >/dev/null || die "sha256sum is missing."

    if [[ "$base" == http* ]] && ! curl -fsS --max-time 10 -o /dev/null https://archlinux.org; then
        die "No internet connection.
       Wi-Fi:  iwctl station wlan0 connect \"<network>\"   (see: iwctl device list)
       Then run this command again."
    fi

    local dir
    dir="$(mktemp -d /tmp/bindle.XXXXXX)"
    say "Downloading Bindle ($version)…"
    curl -fL --retry 3 --progress-bar -o "$dir/bindle" "$base/bindle-x86_64"
    curl -fsSL --retry 3 -o "$dir/bindle-x86_64.sha256" "$base/bindle-x86_64.sha256"
    # Guards against a truncated or corrupted download (same origin, not a signature).
    (cd "$dir" && sed 's#\(  \*\?\).*#\1bindle#' bindle-x86_64.sha256 | sha256sum -c --quiet -) \
        || die "Checksum mismatch; the download is corrupt. Try again."
    chmod 755 "$dir/bindle"

    say "Starting Bindle…"
    # When piped (curl … | bash) stdin is this script; the wizard needs the terminal.
    if [[ ! -t 0 ]] && { : </dev/tty; } 2>/dev/null; then
        exec "$dir/bindle" iso "$@" </dev/tty
    fi
    exec "$dir/bindle" iso "$@"
}

main "$@"
