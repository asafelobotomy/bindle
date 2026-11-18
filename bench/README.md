# Benchmarks

What is measured, with tools from the Rust and Arch ecosystems only:

| Layer | Tool | Metrics |
|---|---|---|
| CLI commands | [hyperfine](https://github.com/sharkdp/hyperfine) 2 | median wall time and peak memory of `--version`, `list`, `detect --json`, `plan --profile desktop --json`, `status --json`, `upgrade --help` |
| Hot paths | [criterion](https://github.com/bheisler/criterion.rs) | catalog load, plan resolution per profile (`bindle-core`), `checkupdates` and Arch news parsing (`bindle-upgrade`) |
| Size | `stat` | release binary size |

Run it yourself (needs `hyperfine`, and a release build):

```sh
cargo build --release -p bindle-cli
bench/run.py --out bench-results.json     # --quick for a fast local run
cargo bench -p bindle-core                # criterion alone, with its own comparison to your last run
```

Everything is offline and read-only, with a throwaway `HOME`.

## In CI

The `bench` job in `ci.yml` builds the base of a pull request and the pull request on the same runner,
runs both twice, interleaved, and compares the medians (`bench/compare.py`, limits in
`bench/thresholds.json`). It writes the table to the job summary and fails the check when a metric
grows past its `fail` limit. A push to `main` uploads its numbers as the `bench-results` artifact.
Absolute numbers differ between machines; only the base-versus-head comparison on one runner is trusted.

To change a limit, edit `thresholds.json`; the first matching rule wins.
