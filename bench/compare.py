#!/usr/bin/env python3
"""Compare benchmark results from the same machine and flag regressions.

    bench/compare.py --base a1.json a2.json --head b1.json b2.json [--summary FILE] [--markdown FILE]

Each side takes one or more result files (bench/run.py or the uQ live bench); the median per
metric is compared, so interleaving two runs per side (base, head, base, head) damps CI noise.
Every metric is "lower is better". A metric regresses when it grows past `warn` (flagged) or `fail`
(exit status 1) as a fraction, and by more than `min_abs` in its own unit, which keeps a 1 ms
command from failing a PR over 0.2 ms of noise. Rules live in bench/thresholds.json: the first
entry whose "match" (a regex on the metric name) fits wins. A metric only one side has is listed,
never failed, so a PR that adds a benchmark passes against a base that lacks it.
"""
import argparse, json, os, re, statistics, sys


def load(paths):
    runs = [json.load(open(p))["metrics"] for p in paths]
    out = {}
    for name in set().union(*runs):
        vals = [r[name]["value"] for r in runs if name in r]
        unit = next(r[name]["unit"] for r in runs if name in r)
        out[name] = (statistics.median(vals), unit)
    return out


def rule(rules, name):
    for r in rules:
        if re.search(r["match"], name):
            return r
    return {"warn": 0.10, "fail": 0.25, "min_abs": 0}


def fmt(v, unit):
    if unit == "ns":
        return f"{v / 1e6:.3f} ms" if v >= 1e6 else f"{v / 1e3:.1f} µs"
    if unit == "bytes":
        return f"{v / 1048576:.2f} MiB"
    return {"pct": f"{v:.2f} %", "per_s": f"{v:.1f} /s", "per_min": f"{v:.1f} /min", "count": f"{v:.0f}"}.get(unit, f"{v:.2f} {unit}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", nargs="+", required=True)
    ap.add_argument("--head", nargs="+", required=True)
    ap.add_argument("--rules", default=os.path.join(os.path.dirname(__file__), "thresholds.json"))
    ap.add_argument("--summary", help="append the markdown table to this file (GITHUB_STEP_SUMMARY)")
    ap.add_argument("--markdown", help="write the markdown table here")
    a = ap.parse_args()
    rules = json.load(open(a.rules))["rules"]
    base, head = load(a.base), load(a.head)

    rows, failed = [], []
    for name in sorted(set(base) | set(head)):
        if name not in base or name not in head:
            rows.append((name, "·", "new" if name in head else "gone", "", "not on both sides"))
            continue
        (b, unit), (h, _) = base[name], head[name]
        r = rule(rules, name)
        # From zero any growth is infinite in relative terms; min_abs then decides if it matters.
        delta = (h - b) / b if b else (float("inf") if h > b else 0.0)
        mark = "ok"
        if abs(h - b) > r.get("min_abs", 0):
            if delta > r["fail"]:
                mark = "FAIL"
                failed.append(name)
            elif delta > r["warn"]:
                mark = "warn"
            elif delta < -r["warn"]:
                mark = "faster"
        rows.append((name, fmt(b, unit), fmt(h, unit), ("new" if delta == float("inf") else f"{delta * 100:+.1f}%"), mark))

    md = ["| metric | base | head | change | |", "|---|---:|---:|---:|---|"]
    md += [f"| `{n}` | {b} | {h} | {d} | {m} |" for n, b, h, d, m in rows]
    text = "\n".join(md) + "\n"
    print(text)
    if a.markdown:
        open(a.markdown, "w").write(text)
    if a.summary:
        with open(a.summary, "a") as f:
            f.write("### Benchmarks: base vs this change\n\n" + text)
    if failed:
        print(f"::error::regressed past the limit: {', '.join(failed)}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
