#!/usr/bin/env python3
"""Check the pinned GitHub releases in catalog/arch/*.toml (`[[module.release]]`).

Bindle pins only stable, CI-green tagged releases. For every pin this fails when:
- the tag's release is a draft or a pre-release (nothing BETA, ALPHA, RC or dev);
- for our own apps (OWN, the uQ family), a check on the tag's commit failed, or the
  commit has no checks at all. A third-party app's CI covers platforms Bindle doesn't
  install on (FreeBSD, macOS), so a red check there is reported, not failed;
- the asset is missing from the release, or its digest is not the pinned sha256.

It also reports a newer release that passes the same rules. With `--bump` it
rewrites those pins in place (tag, asset name and sha256 from the release's
asset digest); review the diff, then add a CHANGELOG line.

Online: reads the GitHub API. Set GITHUB_TOKEN to avoid its low anonymous limit.
"""
import json, os, pathlib, re, sys, urllib.request

repo_root = pathlib.Path(__file__).resolve().parent.parent
catalog = repo_root / "catalog" / "arch"
API = "https://api.github.com"
OK = {"success", "skipped", "neutral"}
OWN = "asafelobotomy/"


def get(path: str):
    req = urllib.request.Request(API + path, headers={"Accept": "application/vnd.github+json", "User-Agent": "bindle-check-pins"})
    if tok := os.environ.get("GITHUB_TOKEN"):
        req.add_header("Authorization", f"Bearer {tok}")
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.load(r)
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return None
        raise


def version(tag: str) -> tuple:
    return tuple(int(n) for n in re.findall(r"\d+", tag))


def ci_problem(repo: str, ref: str) -> str | None:
    """Why the commit at `ref` is not CI-green, or None when it is."""
    runs = (get(f"/repos/{repo}/commits/{ref}/check-runs?per_page=100") or {}).get("check_runs", [])
    if not runs:
        return "no CI checks on its commit"
    bad = [f"{r['name']}: {r['conclusion'] or r['status']}" for r in runs if r["conclusion"] not in OK]
    return "CI not green (" + ", ".join(bad) + ")" if bad else None


def release_problem(rel: dict | None) -> str | None:
    if rel is None:
        return "no release for this tag"
    if rel["draft"]:
        return "the release is a draft"
    if rel["prerelease"]:
        return "the release is marked pre-release"
    return None


def digest(rel: dict, asset: str) -> str | None:
    for a in rel.get("assets", []):
        if a["name"] == asset:
            return (a.get("digest") or "").removeprefix("sha256:") or "unknown"
    return None


def main() -> int:
    bump = "--bump" in sys.argv[1:]
    failures, warnings, newer = [], [], []
    for f in sorted(catalog.glob("*.toml")):
        text = f.read_text()
        for m in re.finditer(r'\[\[module\.release\]\]\nrepo = "([^"]+)"\ntag = "([^"]+)"\nasset = "([^"]+)"\nsha256 = "([0-9a-f]+)"', text):
            repo, tag, asset, sha = m.groups()
            where = f"{repo} {tag}"
            rel = get(f"/repos/{repo}/releases/tags/{tag}")
            own = repo.startswith(OWN)
            problem = release_problem(rel)
            if not problem and (ci := ci_problem(repo, tag)):
                if own:
                    problem = ci
                else:
                    warnings.append(f"{where}: {ci} (third-party; not failed)")
            if not problem:
                d = digest(rel, asset)
                if d is None:
                    problem = f"asset {asset} is not in the release"
                elif d not in ("unknown", sha):
                    problem = f"asset {asset} has sha256 {d}, the catalog pins {sha}"
            if problem:
                failures.append(f"{where}: {problem}")
            # The newest release newer than the pin that passes the same rules.
            for cand in sorted(get(f"/repos/{repo}/releases?per_page=30") or [], key=lambda r: version(r["tag_name"]), reverse=True):
                if version(cand["tag_name"]) <= version(tag):
                    break
                if release_problem(cand) or (own and ci_problem(repo, cand["tag_name"])):
                    continue
                old_v, new_v = tag.lstrip("v"), cand["tag_name"].lstrip("v")
                new_asset = asset.replace(old_v, new_v)
                d = digest(cand, new_asset)
                if d in (None, "unknown"):
                    newer.append(f"{where} -> {cand['tag_name']}: no asset named {new_asset} with a digest; bump it by hand")
                    break
                newer.append(f"{where} -> {cand['tag_name']} ({new_asset})")
                if bump:
                    text = text.replace(m.group(0), m.group(0).replace(f'tag = "{tag}"', f'tag = "{cand["tag_name"]}"').replace(f'asset = "{asset}"', f'asset = "{new_asset}"').replace(sha, d))
                break
        if bump:
            f.write_text(text)
    for n in newer:
        print(("bumped " if bump else "newer stable release: ") + n)
    for w in warnings:
        print("note: " + w)
    for x in failures:
        print("FAIL " + x, file=sys.stderr)
    print(f"{len(failures)} pin problems, {len(newer)} newer stable releases")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
