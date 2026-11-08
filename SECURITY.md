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

Every release carries `SHA256SUMS` and a detached GPG signature
`SHA256SUMS.sig`, made in CI with the Bindle release key:

    75C4 0FA1 27FB F8D4 FBF9  E710 FAC4 0EFF E02D 69FC   Bindle release signing (ed25519, expires 2028-11-07)

The public key is [packaging/bindle-release.asc](packaging/bindle-release.asc);
`install.sh` embeds it and refuses anything it didn't sign. See
[docs/ISO.md](docs/ISO.md#trust) to verify by hand. If the key ever has to be
replaced, the new one will be signed by this one and announced in the
CHANGELOG.
