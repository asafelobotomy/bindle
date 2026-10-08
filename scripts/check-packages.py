#!/usr/bin/env python3
"""Verify every repo package named in catalog/ exists in the pacman sync DBs.

Run `sudo pacman -Sy` (or wait for a timer) first; a stale DB reports
recently added packages as missing. AUR packages (`aur = [...]`) are skipped.
"""
import pathlib, subprocess, sys, tomllib

root = pathlib.Path(__file__).resolve().parent.parent / "catalog"
names = set()
for f in (root / "arch").glob("*.toml"):
    for m in tomllib.loads(f.read_text()).get("module", []):
        names.update(m.get("packages", []))
for d in tomllib.loads((root / "desktops.toml").read_text())["desktop"]:
    for key in ("essentials", "nm", "bt"):  # "archinstall" is upstream data, matched not installed
        names.update(d.get(key, []))


def exists(name: str) -> bool:
    return any(subprocess.run(["pacman", flag, name], capture_output=True).returncode == 0 for flag in ("-Si", "-Sg"))

missing = sorted(n for n in names if not exists(n))
print(f"checked {len(names)} packages")
if missing:
    print("missing from sync DBs:", " ".join(missing))
    sys.exit(1)
