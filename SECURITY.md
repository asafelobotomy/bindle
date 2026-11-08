# Security policy

Bindle runs a root helper, edits system configuration, and its ISO installer
wipes disks, so security reports are taken seriously.

## Reporting

Please report vulnerabilities privately through GitHub's
**Security → Report a vulnerability** on this repository, not in a public
issue. Include the Bindle version (`bindle --version`), what you ran, and what
happened. You should get a reply within a week.

## Scope

In scope: the root helper and its protocol (`crates/bindle-cli/src/apply/`),
anything that writes as root, the first-login autostart, `install.sh`, the
archinstall plugin, and release artifacts.

The helper is deliberately as powerful as `sudo` for the user who
authenticated (see `docs/APPLY.md`, "Trust model"); escaping that boundary, or
making Bindle act on another user's behalf, is in scope.

## Verifying releases

The ISO binary ships with a `.sha256` file from the same release, so for now it
only detects corrupted downloads. GPG-signed checksums are planned; the
signing key's fingerprint will be listed here.
