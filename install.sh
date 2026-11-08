#!/usr/bin/env bash
# Bindle: install Arch Linux from the live ISO with Bindle's presets, then
# finish setting up on first login.
#
#   bash <(curl -fsSL https://raw.githubusercontent.com/asafelobotomy/bindle/main/install.sh)
#   curl -fsSL https://raw.githubusercontent.com/asafelobotomy/bindle/main/install.sh | bash
#
# Arguments are passed to `bindle iso` (e.g. --desktop plasma --profile gaming).
# Environment:
#   BINDLE_VERSION   release tag to fetch, e.g. v0.2.0 (default: latest)
#   BINDLE_REPO      GitHub owner/repo (default: asafelobotomy/bindle)
#   BINDLE_BASE_URL  override the download location entirely (testing/mirrors);
#                    downloads are still verified against the signing key below
#   BINDLE_INSECURE=1  allow a non-https BINDLE_BASE_URL (http://, file://;
#                    local testing only; the signature is still checked)
#   BINDLE_FORCE=1   skip the live-ISO check; only together with --dry-run
#
# The binary is checked against SHA256SUMS, and SHA256SUMS against a detached
# signature by the release key embedded here, so a tampered release fails.
set -euo pipefail

# Release signing key: packaging/bindle-release.asc (see SECURITY.md).
SIGNING_FPR=75C40FA127FBF8D4FBF9E710FAC40EFFE02D69FC
SIGNING_KEY='-----BEGIN PGP PUBLIC KEY BLOCK-----

mDMEavB/8xYJKwYBBAHaRw8BAQdAhhi5TJ4cbluXe608n8nowqL4xd0wKR8TfMLa
2MA7H2i0FkJpbmRsZSByZWxlYXNlIHNpZ25pbmeIlgQTFgoAPhYhBHXED6En+/jU
+/nnEPrEDv/gLWn8BQJq8H/zAhsDBQkDwmcABQsJCAcCBhUKCQgLAgQWAgMBAh4B
AheAAAoJEPrEDv/gLWn8V5AA/jTNac8lR5rqhx8wREVB7Y9rAwBHkSUodT2Vm/c/
5q+PAP4seYjXOD8k5UTyMDrPebJnk0f92K/U5YopuIJG3VyWDA==
=m6zh
-----END PGP PUBLIC KEY BLOCK-----'

main() {
    local repo="${BINDLE_REPO:-asafelobotomy/bindle}"
    local version="${BINDLE_VERSION:-latest}"
    say() { printf '\033[1;33m::\033[0m %s\n' "$*"; }
    die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

    [[ "$version" =~ ^(latest|v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?)$ ]] || die "BINDLE_VERSION must be like v0.2.0 (got: $version)"
    [[ "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die "BINDLE_REPO must be owner/repo (got: $repo)"
    local dry_run=0 a
    for a in "$@"; do [[ "$a" == --dry-run ]] && dry_run=1; done
    if [[ "${BINDLE_FORCE:-0}" == 1 && $dry_run == 0 ]]; then
        die "BINDLE_FORCE=1 is only for --dry-run; a real install runs from the live ISO."
    fi

    [[ "$(uname -m)" == x86_64 ]] || die "Bindle supports x86_64 only."
    if [[ ! -d /run/archiso && "${BINDLE_FORCE:-0}" != 1 ]]; then
        die "This installs Arch Linux and wipes disks. Run it from the Arch Linux live ISO.
       On an installed system, download the Bindle AppImage instead."
    fi
    [[ "${BINDLE_FORCE:-0}" == 1 || "$(id -u)" -eq 0 ]] || die "Run as root (the live ISO shell already is)."
    local tool
    for tool in curl sha256sum gpg gpgv; do
        command -v "$tool" >/dev/null || die "$tool is missing."
    done

    local curl=(curl -fL --retry 3 --proto '=https' --tlsv1.2)
    local base
    if [[ -n "${BINDLE_BASE_URL:-}" ]]; then
        base="$BINDLE_BASE_URL"
        if [[ "$base" != https://* ]]; then
            [[ "${BINDLE_INSECURE:-0}" == 1 ]] || die "BINDLE_BASE_URL isn't https://; use https:// (or BINDLE_INSECURE=1 for local testing)."
            curl=(curl -fL --retry 3)
        fi
    else
        if ! curl -fsS --proto '=https' --tlsv1.2 --max-time 10 -o /dev/null https://archlinux.org; then
            die "No internet connection.
       Wi-Fi:  iwctl station wlan0 connect \"<network>\"   (see: iwctl device list)
       Then run this command again."
        fi
        if [[ "$version" == latest ]]; then
            # Pin one tag so every file comes from the same release.
            local url
            url="$("${curl[@]}" -sS -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")"
            version="${url##*/}"
            [[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || die "could not find the latest release (got: $url)"
        fi
        base="https://github.com/$repo/releases/download/$version"
    fi

    local dir
    dir="$(mktemp -d /tmp/bindle.XXXXXX)"
    trap 'rm -rf "$dir"' EXIT
    say "Downloading Bindle ($version)…"
    "${curl[@]}" --progress-bar -o "$dir/bindle" "$base/bindle-x86_64"
    "${curl[@]}" -sS -o "$dir/SHA256SUMS" "$base/SHA256SUMS"
    "${curl[@]}" -sS -o "$dir/SHA256SUMS.sig" "$base/SHA256SUMS.sig" \
        || die "This release has no signature (SHA256SUMS.sig); use a signed release."
    verify "$dir" || die "Verification failed: this download is not the signed Bindle release. Nothing was run."
    chmod 755 "$dir/bindle"

    say "Starting Bindle…"
    trap - EXIT  # the binary runs from $dir
    # When piped (curl … | bash) stdin is this script; the wizard needs the terminal.
    if [[ ! -t 0 ]] && { : </dev/tty; } 2>/dev/null; then
        exec "$dir/bindle" iso "$@" </dev/tty
    fi
    exec "$dir/bindle" iso "$@"
}

# SHA256SUMS must carry a valid signature by exactly SIGNING_FPR, and list
# the downloaded binary with a matching checksum.
verify() {
    local dir="$1"
    mkdir -m 700 "$dir/gnupg"
    printf '%s\n' "$SIGNING_KEY" | GNUPGHOME="$dir/gnupg" gpg --batch --quiet --dearmor >"$dir/release.gpg" || return 1
    GNUPGHOME="$dir/gnupg" gpgv --status-fd 1 --keyring "$dir/release.gpg" "$dir/SHA256SUMS.sig" "$dir/SHA256SUMS" 2>/dev/null \
        | grep -q "^\[GNUPG:\] VALIDSIG $SIGNING_FPR " || return 1
    local sum
    sum="$(awk '$2 == "bindle-x86_64" || $2 == "*bindle-x86_64" { print $1 }' "$dir/SHA256SUMS")"
    [[ "$sum" =~ ^[0-9a-f]{64}$ ]] || return 1
    [[ "$(sha256sum <"$dir/bindle" | cut -d' ' -f1)" == "$sum" ]]
}

main "$@"
