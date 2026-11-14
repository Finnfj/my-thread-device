#!/usr/bin/env bash
# ADR version guard. Run by CI and by /upgrade-lockstep-set; works in Git Bash
# on Windows too.
#
# An ADR that rests on what a dependency can or can't do marks that claim with
# "(checked in <crate> <version>)" or "(checked in <crate> @<rev>)", for example
# "(checked in rs-matter 0.4.1)" or "(checked in rs-matter-embassy @2a806b0)".
# This guard fails when a marker no longer matches Cargo.lock, so an upgrade
# can't leave a claim behind unchecked. To clear it, re-check the claim against
# the new version, fix the ADR if it changed, then update the marker.
set -euo pipefail

cd "$(dirname "$0")/.."

# "name version source" for every package in Cargo.lock ("-" when no source).
lock=$(awk '
  { sub(/\r$/, "") }
  /^\[\[package\]\]/ { if (n) print n, v, (s ? s : "-"); n = v = s = "" }
  /^name = /    { n = $3; gsub(/"/, "", n) }
  /^version = / { v = $3; gsub(/"/, "", v) }
  /^source = /  { s = $3; gsub(/"/, "", s) }
  END { if (n) print n, v, (s ? s : "-") }
' Cargo.lock)

markers=$(grep -rnoE 'checked in [a-z0-9_-]+ (@[0-9a-f]{7,40}|[0-9][0-9A-Za-z.+-]*[0-9A-Za-z])' docs/adr || true)

stale=0
while IFS= read -r m; do
  [ -z "$m" ] && continue
  loc=${m%%:checked in *}   # file:line
  set -- ${m#*checked in }  # $1 = crate, $2 = version or @rev
  crate=$1 want=$2
  case $want in
    @*) have=$(echo "$lock" | awk -v c="$crate" -v r="#${want#@}" '$1 == c && index($3, r) { print "ok" }') ;;
    *)  have=$(echo "$lock" | awk -v c="$crate" -v w="$want" '$1 == c && $2 == w { print "ok" }') ;;
  esac
  if [ -z "$have" ]; then
    now=$(echo "$lock" | awk -v c="$crate" '$1 == c { s = $3; sub(/^.*#/, "@", s); print ($3 ~ /^git/ ? $2 " " substr(s, 1, 8) : $2) }' | paste -sd, -)
    echo "::error file=${loc%%:*},line=${loc##*:}::$crate $want is marked as checked, Cargo.lock has ${now:-no $crate}"
    stale=1
  fi
done <<< "$markers"

if [ "$stale" -ne 0 ]; then
  echo "Re-check each claim above against the version in Cargo.lock, then update its marker."
  exit 1
fi
echo "All $(echo "$markers" | grep -c . || true) ADR version markers match Cargo.lock."
