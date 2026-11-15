#!/usr/bin/env bash
# Make the next Bindle release signing key (see SECURITY.md, "Key rotation").
#
#   scripts/new-release-key.sh [VAULT]     # Proton Pass vault, default: Personal
#
# The private key goes straight into a Proton Pass note ("Bindle release signing
# key <fingerprint>") and is never printed or written outside a temporary 0700
# directory, removed on exit. The public key is added to
# packaging/bindle-release.asc, which then holds both keys as one block.
# Needs gpg, jq and pass-cli (logged in).
set -euo pipefail
cd "$(dirname "$0")/.."
vault="${1:-Personal}"
for tool in gpg jq pass-cli; do
    command -v "$tool" >/dev/null || { echo "$tool is missing" >&2; exit 1; }
done
pass-cli info >/dev/null 2>&1 || { echo "log in first: pass-cli login" >&2; exit 1; }

dir="$(mktemp -d)"
chmod 700 "$dir"
trap 'rm -rf "$dir"' EXIT
export GNUPGHOME="$dir/gnupg"
mkdir -m 700 "$GNUPGHOME"

# No passphrase: CI imports it with --batch. Its protection is where it's kept.
gpg --batch --quiet --pinentry-mode loopback --passphrase '' \
    --quick-gen-key "Bindle release signing" ed25519 sign 3y
fpr="$(gpg --list-keys --with-colons | awk -F: '/^fpr/ { print $10; exit }')"
expires="$(gpg --list-keys --with-colons | awk -F: '/^pub/ { print $7; exit }')"

gpg --batch --armor --export-secret-keys "$fpr" >"$dir/private.asc"
jq -n --arg title "Bindle release signing key $fpr" --rawfile note "$dir/private.asc" \
    '{title: $title, note: $note, extra_fields: []}' >"$dir/item.json"
pass-cli item create note --vault-name "$vault" --from-template "$dir/item.json" >/dev/null
echo "Saved the private key in Proton Pass ($vault): \"Bindle release signing key $fpr\""

# One armored block with the current key(s) and the new one.
gpg --batch --quiet --import packaging/bindle-release.asc
gpg --batch --armor --export-options export-minimal --export >packaging/bindle-release.asc.new
mv packaging/bindle-release.asc.new packaging/bindle-release.asc
echo "Added the public key to packaging/bindle-release.asc"
echo "New fingerprint: $fpr (expires $(date -ud "@$expires" +%F))"
