"""Print what upstream Lockstep Set releases and commits require.

  python -I upstream_reqs.py crate esp-radio            # recent versions, yanked marked
  python -I upstream_reqs.py crate esp-radio 1.0.0-beta.1   # that version's seam deps
  python -I upstream_reqs.py git https://github.com/sysgrok/rs-matter-embassy \
      --manifest rs-matter-embassy/Cargo.toml f31233a..HEAD
      # per commit: seam deps of that manifest + active [patch.crates-io] of the root

Only dependencies that matter to the Lockstep Set are printed (see SEAM below).
Git repos are bare-cloned once into --cache (default: a temp folder).
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import tomllib
import urllib.request

SEAM = re.compile(r"^(esp-|embassy-(executor|sync)$|bt-hci|trouble-host|nimble-rs|openthread|rs-matter)")


def index_path(name):
    n = name.lower()
    if len(n) <= 2:
        return f"{len(n)}/{n}"
    if len(n) == 3:
        return f"3/{n[0]}/{n}"
    return f"{n[:2]}/{n[2:4]}/{n}"


def crate(name, version, last):
    req = urllib.request.Request(
        f"https://index.crates.io/{index_path(name)}",
        headers={"User-Agent": "upgrade-lockstep-set (upstream_reqs.py)"},
    )
    entries = [json.loads(line) for line in urllib.request.urlopen(req).read().decode().splitlines()]
    if not version:
        for e in entries[-last:]:
            print(e["vers"], "(yanked)" if e["yanked"] else "")
        return
    for e in entries:
        if e["vers"] == version:
            print(name, version, "(yanked)" if e["yanked"] else "")
            feats = {**e["features"], **e.get("features2", {})}
            seen = set()
            for d in e["deps"]:
                dep = d.get("package") or d["name"]
                if d["kind"] != "dev" and SEAM.match(dep) and (dep, d["req"]) not in seen:
                    seen.add((dep, d["req"]))
                    # No `dep:` reference means Cargo's implicit feature named after the dep.
                    enabled_by = [f for f, v in feats.items() if f"dep:{d['name']}" in v] or [d["name"]]
                    note = f"  optional, enabled by {enabled_by}" if d["optional"] else ""
                    print(f"  {dep} {d['req']}{note}")
            return
    sys.exit(f"{name} {version} not on crates.io")


def git(url, manifest, rev_range, cache):
    repo = os.path.join(cache, re.sub(r"\W", "_", url) + ".git")
    if os.path.isdir(repo):
        subprocess.run(["git", "-C", repo, "fetch", "-q", "origin", "+refs/heads/*:refs/heads/*"], check=True)
    else:
        subprocess.run(["git", "clone", "-q", "--bare", url, repo], check=True)
    print(f"# clone: {repo}")

    def show(rev, path):
        r = subprocess.run(["git", "-C", repo, "show", f"{rev}:{path}"], capture_output=True)
        return tomllib.loads(r.stdout.decode()) if r.returncode == 0 else {}

    def req(spec, ws):
        if isinstance(spec, dict) and spec.get("workspace"):
            spec = ws or {}
        if isinstance(spec, str):
            return spec
        return spec.get("version") or spec.get("git", "") + (f" rev={spec['rev']}" if "rev" in spec else "")

    revs = subprocess.run(
        ["git", "-C", repo, "log", "--reverse", "--format=%h %cd %s", "--date=short", rev_range],
        capture_output=True, text=True, check=True,
    ).stdout.splitlines()
    for line in revs:
        rev = line.split()[0]
        m, root = show(rev, manifest), show(rev, "Cargo.toml")
        ws_deps = root.get("workspace", {}).get("dependencies", {})
        deps = {**m.get("workspace", {}).get("dependencies", {}), **m.get("dependencies", {})}
        seam = [f"{k} {req(v, ws_deps.get(k))}" for k, v in deps.items() if SEAM.match(k)]
        patches = list(root.get("patch", {}).get("crates-io", {}))
        print(line[:90])
        print("   deps:   ", ", ".join(seam) or "-")
        print("   patches:", ", ".join(patches) or "-")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("crate")
    c.add_argument("name")
    c.add_argument("version", nargs="?")
    c.add_argument("--last", type=int, default=6)
    g = sub.add_parser("git")
    g.add_argument("url")
    g.add_argument("rev_range", help="OLD..HEAD, or REV^! for one commit")
    g.add_argument("--manifest", default="Cargo.toml", help="crate manifest inside the repo")
    g.add_argument("--cache", default=os.path.join(tempfile.gettempdir(), "upstream_reqs"))
    a = p.parse_args()
    if a.cmd == "crate":
        crate(a.name, a.version, a.last)
    else:
        os.makedirs(a.cache, exist_ok=True)
        git(a.url, a.manifest, a.rev_range, a.cache)


if __name__ == "__main__":
    main()
