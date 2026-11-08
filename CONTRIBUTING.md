# Contributing

Read [AGENTS.md](AGENTS.md) for the layout and conventions; the short version:

```bash
make test      # unit + CLI tests (catalog validation runs here)
make lint      # rustfmt --check + clippy -D warnings
cargo deny check
python3 scripts/check-packages.py   # on Arch, after pacman -Sy (and -Fy for units)
```

- Never test `apply`, `undo` or `first-login install` on your own system: use
  `--sandbox <dir>` / `--root <dir>`, or an Arch VM (`tests/vm/run.sh`,
  [docs/VM-TEST.md](docs/VM-TEST.md)). `bindle iso` only runs from the live ISO;
  use `--dry-run`.
- Keep `plan::resolve` pure; anything that touches the system belongs in the
  executor. Hardware-dependent logic is a builtin with unit tests driven by `Facts`.
- Source files stay at or under 400 lines (`tests/loc.rs` enforces it).
- The workspace must build with the `rust-version` in `Cargo.toml` (CI's
  `msrv` job).
- New catalog modules (including `[[module.release]]` assets): see [docs/CATALOG.md](docs/CATALOG.md). The loader
  validates them in `cargo test`.
- Note user-visible changes in [CHANGELOG.md](CHANGELOG.md).

Security issues: see [SECURITY.md](SECURITY.md).
