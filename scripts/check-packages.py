#!/usr/bin/env python3
"""Verify every repo package Bindle can install exists in the pacman sync DBs.

Covers catalog modules, desktops.toml (essentials/nm/bt/greeter/archinstall)
and the `PACKAGES` const lists in crates/bindle-core/src/builtins/.

Run `sudo pacman -Sy` (or wait for a timer) first; a stale DB reports
recently added packages as missing. AUR packages (`aur = [...]`) are skipped.
"""
import pathlib, re, subprocess, sys, tomllib

repo = pathlib.Path(__file__).resolve().parent.parent
root = repo / "catalog"
names = set()
# archinstall's own package lists are passed to pacman verbatim, so a name
# only needs to resolve (it may be a provide, e.g. ttf-freefont).
provided = set()
for f in (root / "arch").glob("*.toml"):
    for m in tomllib.loads(f.read_text()).get("module", []):
        names.update(m.get("packages", []))
for d in tomllib.loads((root / "desktops.toml").read_text())["desktop"]:
    for key in ("essentials", "nm", "bt"):
        names.update(d.get(key, []))
    if d.get("broken"):  # archinstall's own list is known-bad; never offered
        continue
    if d.get("greeter"):
        names.add(d["greeter"])
    provided.update(d.get("archinstall", []))

const = re.compile(r"pub const PACKAGES: &\[&str\] = &\[(.*?)\];", re.S)
for src in sorted((repo / "crates/bindle-core/src/builtins").glob("*.rs")):
    for body in const.findall(src.read_text()):
        names.update(re.findall(r'"([^"]+)"', body))
provided -= names


def ok(args: list[str]) -> bool:
    return subprocess.run(["pacman", *args], capture_output=True).returncode == 0


def exists(name: str) -> bool:
    return ok(["-Si", name]) or ok(["-Sg", name])


def resolves(name: str) -> bool:
    return exists(name) or ok(["-Sp", "--print-format", "%n", name])


missing = sorted(n for n in names if not exists(n)) + sorted(n for n in provided if not resolves(n))
print(f"checked {len(names) + len(provided)} packages")
if missing:
    print("missing from sync DBs:", " ".join(missing))
    sys.exit(1)
