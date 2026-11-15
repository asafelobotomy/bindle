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
`SHA256SUMS.sig`, made in CI with a Bindle release key:

    75C4 0FA1 27FB F8D4 FBF9  E710 FAC4 0EFF E02D 69FC   Bindle release signing (ed25519, expires 2028-11-07)
    CCFF 34EE 4CC0 AE7C C686  5E92 7C23 0DE2 DE41 DF9C   Bindle release signing (ed25519, expires 2029-11-14)

The first signs releases up to v0.1.9; the second, added in v0.1.9, signs
the ones after it (see "Key rotation"). The public keys are
[packaging/bindle-release.asc](packaging/bindle-release.asc); `install.sh`
embeds them and refuses anything neither signed, and Bindle's self-update
(`bindleupgrade`) has them built in. See
[docs/ISO.md](docs/ISO.md#trust) to verify by hand.

From v0.1.9 every release also carries `VERSION`: exactly its tag and a
newline (`v0.1.9`), listed in `SHA256SUMS`, so the signature covers which
release it is. `install.sh` requires it when it looks up the latest release
and checks it names that tag; self-update requires it to name the tag it is
updating to, and that tag to be newer than the running Bindle. A signed but
older release served in place of the latest is refused. (Releases up to
v0.1.8 have no `VERSION`; `install.sh` pinned to one with `BINDLE_VERSION`
warns and goes on. Self-update from them still works: the new Bindle's
release has `VERSION`, and the old Bindle doesn't look for it.)

## How releases are made

`.github/workflows/release.yml` runs on a `v*` tag, in three jobs so the
signing key never shares a machine with the build's dependencies:

1. **build**: an `archlinux:base-devel` container pinned by digest, a
   read-only token and no persisted git credentials. It installs the build
   packages, runs the tests, builds the binaries and the AppImage, and uploads
   `dist/` as a workflow artifact. Nothing here can sign or publish.
2. **sign**: a fresh runner with no checkout and no cargo, in the `release`
   environment. It downloads that artifact, writes `VERSION` and
   `SHA256SUMS`, imports `BINDLE_GPG_KEY`, signs, checks the signature is by
   the fingerprint above, and uploads the three files. No token permissions.
3. **publish**: the only job with `contents: write`; it checks every file
   against the signed `SHA256SUMS` and creates the GitHub release.

A manual run (`workflow_dispatch`) builds an existing tag as an artifact. It
signs and publishes only when the input is a real tag naming the commit it
built (`git rev-parse --verify "refs/tags/$TAG^{commit}"`) and `publish` is
set.

## Key rotation

Installed Bindles trust the fingerprints built into them (`SIGNING_FPRS` in
`crates/bindle-upgrade/src/selfupdate.rs`), and `install.sh` the ones it
embeds (`SIGNING_FPRS`, the same list as the release workflow's; a unit test
checks the three agree). So a new key has to reach users before it is used.
`scripts/new-release-key.sh` makes one, saves the private half in Proton Pass
and adds the public half to `packaging/bindle-release.asc`.

**Current rotation:** v0.1.9 added `CCFF34EE…DE41DF9C`, made after the first
key's private half turned out to exist only as the GitHub secret. v0.1.9 was
the last release signed by the first key; on 2026-11-15 `BINDLE_GPG_KEY`
became an environment secret in `release` holding the new key, and the
repository secret (the first key's only copy) was deleted. Releases after
v0.1.9 are signed by the new key. The first fingerprint is dropped in a later
release; it can no longer sign anything.

The steps:

1. Well before the expiry, make the next key, sign it with this one, and add
   its fingerprint to `SIGNING_FPRS` and its public key to
   `packaging/bindle-release.asc`; ship that in a release still signed by the
   current key. Announce it in the CHANGELOG.
2. Once that release has been out long enough for installs to update, sign
   with the new key and switch `install.sh` (and the workflow's
   `SIGNING_FPR`) to it.
3. Drop the old fingerprint in a later release.

Extending the current key's expiry is the other option: it keeps the
fingerprint, but installed Bindles carry the public key with the old expiry,
so the re-exported key still has to ship in a release before 2028-11-07
(test that an old Bindle's `gpgv` accepts it before relying on this).

## Repository settings (the maintainer's to make)

These live in GitHub's settings, not in the repository, and should be on:

- **A ruleset on `main`**: changes by pull request only, the `ci` checks
  required to pass, no force-pushes, no deletion.
- **A tag ruleset for `v*`**: only maintainers can create tags, and none can
  be moved or deleted (a release tag names one commit for good).
- **The `release` environment**, with required reviewers and deployments
  limited to `v*` tags, holding `BINDLE_GPG_KEY` as an environment secret
  (and no repository secret of that name). The sign job runs in this
  environment; repository secrets reach environment jobs too, so only an
  environment secret makes signing wait for a reviewer's approval. A manual
  run that signs must be dispatched on the tag (`--ref v0.1.9`), not `main`.
