#!/usr/bin/env python3
"""Verify every repo package Bindle can install exists in the pacman sync DBs.

Covers catalog modules, desktops.toml (essentials/nm/bt/greeter/archinstall)
and the `PACKAGES` const lists in crates/bindle-core/src/builtins/.

Run `sudo pacman -Sy` (or wait for a timer) first; a stale DB reports
recently added packages as missing.

Also checked: systemd units the catalog enables, against pacman's files DB
(`sudo pacman -Fy`; skipped when it is missing), and with `--aur` (online) the
AUR packages via the AUR RPC and the pinned GitHub release assets. A preview
desktop's module (uQ) may not be on the AUR yet; that is reported, not failed,
and once it is there the script says the desktop can be recommended.
"""
import json, pathlib, re, subprocess, sys, tomllib, urllib.parse, urllib.request

repo = pathlib.Path(__file__).resolve().parent.parent
root = repo / "catalog"
names = set()
# archinstall's own package lists are passed to pacman verbatim, so a name
# only needs to resolve (it may be a provide, e.g. ttf-freefont).
provided = set()
units = set()
aur = set()
releases = []
module_aur = {}
for f in (root / "arch").glob("*.toml"):
    for m in tomllib.loads(f.read_text()).get("module", []):
        names.update(m.get("packages", []))
        units.update(m.get("enable", []))
        aur.update(m.get("aur", []))
        module_aur[m["id"]] = set(m.get("aur", []))
        releases += m.get("release", [])
desktops = tomllib.loads((root / "desktops.toml").read_text())["desktop"]
# Desktop id -> AUR packages its module installs, while the desktop is a preview.
preview = {d["id"]: module_aur.get(d.get("module"), set()) for d in desktops if d.get("status") == "preview"}
for d in desktops:
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


def unit_shipped(unit: str) -> bool:
    return ok(["-F", f"usr/lib/systemd/system/{unit}"]) or ok(["-F", f"usr/lib/systemd/user/{unit}"])


def aur_missing(pkgs: set[str]) -> list[str]:
    query = urllib.parse.urlencode([("v", "5"), ("type", "info")] + [("arg[]", p) for p in sorted(pkgs)])
    with urllib.request.urlopen(f"https://aur.archlinux.org/rpc/?{query}", timeout=30) as r:
        found = {res["Name"] for res in json.load(r)["results"]}
    return sorted(pkgs - found)


missing = sorted(n for n in names if not exists(n)) + sorted(n for n in provided if not resolves(n))
print(f"checked {len(names) + len(provided)} packages")
if ok(["-F", "usr/bin/pacman"]):
    missing += [f"unit:{u}" for u in sorted(units) if not unit_shipped(u)]
    print(f"checked {len(units)} units")
else:
    print("no pacman files DB (sudo pacman -Fy); units not checked")
def release_missing(r: dict) -> bool:
    url = f"https://github.com/{r['repo']}/releases/download/{r['tag']}/{r['asset']}"
    req = urllib.request.Request(url, method="HEAD")
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return resp.status != 200
    except Exception:
        return True


if "--aur" in sys.argv[1:]:
    missing += [f"release:{r['repo']}/{r['tag']}/{r['asset']}" for r in releases if release_missing(r)]
    print(f"checked {len(releases)} release assets")
    not_on_aur = set(aur_missing(aur))
    print(f"checked {len(aur)} AUR packages")
    for desktop, pkgs in sorted(preview.items()):
        if pkgs & not_on_aur:
            print(f"preview desktop {desktop}: not on the AUR yet: {' '.join(sorted(pkgs & not_on_aur))}")
            not_on_aur -= pkgs
        elif pkgs:
            print(f"preview desktop {desktop}: {' '.join(sorted(pkgs))} on the AUR; it can be recommended "
                  "(docs/CATALOG.md, \"Recommended desktop\")")
    missing += [f"aur:{p}" for p in sorted(not_on_aur)]
if missing:
    print("missing from sync DBs:", " ".join(missing))
    sys.exit(1)
