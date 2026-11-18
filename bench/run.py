#!/usr/bin/env python3
"""Run Bindle's benchmarks and write one results file.

    bench/run.py [--bin target/release/bindle] [--out bench-results.json] [--quick]

Metrics (all "lower is better"): command wall times from hyperfine (median, ms) with each command's peak memory, the criterion
medians of the engine's and bindleUP's hot paths (ns), and the size of the release binary (bytes).
Needs `hyperfine` and `cargo`. Nothing here touches the real system: every command runs with a
throwaway HOME and XDG dirs, and only offline, read-only commands are timed.
"""
import argparse, json, os, pathlib, subprocess, sys, tempfile

COMMANDS = {
    "cmd_version": "--version",
    "cmd_list": "list",
    "cmd_detect_json": "detect --json",
    "cmd_plan_desktop_json": "plan --profile desktop --json",
    "cmd_status_json": "status --json",
    "cmd_upgrade_help": "upgrade --help",
}
CRITERION = [("bindle-core", "core"), ("bindle-upgrade", "parse")]


def run(cmd, **kw):
    print("+", " ".join(map(str, cmd)), flush=True)
    return subprocess.run(cmd, check=True, **kw)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default="target/release/bindle")
    ap.add_argument("--out", default="bench-results.json")
    ap.add_argument("--quick", action="store_true", help="fewer runs (local smoke test)")
    a = ap.parse_args()
    binary = pathlib.Path(a.bin).resolve()
    metrics = {"binary_size": {"value": binary.stat().st_size, "unit": "bytes"}}

    home = tempfile.mkdtemp(prefix="bindle-bench-")
    env = dict(os.environ, HOME=home, XDG_CONFIG_HOME=home + "/c", XDG_STATE_HOME=home + "/s",
               XDG_CACHE_HOME=home + "/k", XDG_DATA_HOME=home + "/d", LC_ALL="C.UTF-8")
    runs = ["--runs", "10", "--warmup", "2"] if a.quick else ["--runs", "60", "--warmup", "5"]
    for name, args in COMMANDS.items():
        export = pathlib.Path(home) / f"{name}.json"
        run(["hyperfine", *runs, "-N", "--export-json", str(export), f"{binary} {args}"],
            env=env, stdout=subprocess.DEVNULL)
        # hyperfine 2 puts the summary statistics under results[0].summary.
        r = json.loads(export.read_text())["results"][0]["summary"]
        t, mem = r["time_wall_clock"], r["memory_peak_resident"]
        # The median shrugs off the odd slow run on a shared CI machine; the mean does not.
        metrics[name] = {"value": round(t["median"] * 1000, 3), "unit": "ms", "spread": round(t["stddev"] * 1000, 3)}
        metrics[name.replace("cmd_", "rss_")] = {"value": round(mem["median"]), "unit": "bytes"}

    crit = ["--warm-up-time", "1", "--measurement-time", "2"] if a.quick else ["--warm-up-time", "2", "--measurement-time", "5"]
    # A checkout from before the benches existed (the base of the PR that adds them) has none.
    todo = [(c, b) for c, b in CRITERION if pathlib.Path("crates", c, "benches", b + ".rs").exists()]
    for crate, bench in todo:
        run(["cargo", "bench", "--locked", "-p", crate, "--bench", bench, "--", *crit], stdout=subprocess.DEVNULL)
    root = pathlib.Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "criterion"
    for est in sorted(root.glob("*/new/estimates.json")) if todo else []:
        e = json.loads(est.read_text())
        metrics["bench_" + est.parent.parent.name] = {"value": round(e["median"]["point_estimate"], 1), "unit": "ns"}

    pathlib.Path(a.out).write_text(json.dumps({"tool": "bindle", "metrics": metrics}, indent=2) + "\n")
    print(f"wrote {a.out}: {len(metrics)} metrics")


if __name__ == "__main__":
    sys.exit(main())
