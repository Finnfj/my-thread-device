#!/usr/bin/env bash
# Print the source folder of a dependency at the version Cargo.lock pins, so a
# search reads the code this build compiles, not whichever of several versions
# in ~/.cargo/registry sorts first. Works in Git Bash on Windows too.
#
#   bash scripts/dep-src.sh esp-hal               -> esp-hal 1.2.2 <folder>
#   grep -rn WakeLock "$(bash scripts/dep-src.sh -q esp-hal)"
#
# -q prints only the folder. Several lines come back when the graph holds the
# crate in more than one version. Git dependencies resolve to their checkout.
#
# esp-hal's clock tree and per-chip peripheral code (e.g. which drivers take a
# WakeLock) are generated into esp-metadata-generated, not esp-hal: search both.
set -euo pipefail

quiet=0
[ "${1:-}" = "-q" ] && { quiet=1; shift; }
[ $# -eq 1 ] || { echo "usage: $0 [-q] CRATE" >&2; exit 2; }

cd "$(dirname "$0")/.."
py=$(command -v python3 || command -v python) || { echo "$0: needs python" >&2; exit 2; }

cargo metadata --format-version 1 --locked --filter-platform riscv32imac-unknown-none-elf |
  "$py" -c '
import json, os, sys
crate, quiet = sys.argv[1], sys.argv[2] == "1"
meta = json.load(sys.stdin)
resolved = {n["id"] for n in meta["resolve"]["nodes"]}
hits = [p for p in meta["packages"] if p["name"] == crate and p["id"] in resolved]
for p in sorted(hits, key=lambda p: p["version"]):
    path = os.path.dirname(p["manifest_path"]).replace(os.sep, "/")
    print(path if quiet else " ".join((p["name"], p["version"], path)))
if not hits:
    sys.exit(f"{crate}: not in the dependency graph (check the name in Cargo.lock)")
' "$1" "$quiet"
