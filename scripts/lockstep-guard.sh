#!/usr/bin/env bash
# Lockstep Set duplicate guard (ADR-0012). Run by CI and by /upgrade-lockstep-set;
# works in Git Bash on Windows too.
#
# Cargo resolves semver-incompatible versions of a crate side by side, so a
# Lockstep Set crate in two versions builds fine until the types meet. The
# graph has legitimate duplicates elsewhere (embassy-sync, heapless), so only
# the crates whose types cross between Lockstep Set crates must appear once.
#
# SEAM_CRATES is that list. It is not the Lockstep Set list (renovate.json):
# it also holds the crates where the set's members meet. Re-check it on every
# Lockstep Set upgrade with `cargo tree -i <crate>`.
# BLE meets at bt-hci-transport, not bt-hci: esp-radio 1.0.0-beta.1 hands
# trouble-host a bt_hci_transport::Transport, and still pulls bt-hci 0.9 for
# a legacy impl nothing here uses, next to trouble-host's bt-hci 0.10.
# embassy-executor-timer-queue meets through a link symbol, not a type:
# embassy-executor allocates the timer queue items and embassy-time-queue-utils
# (under esp-rtos) fills them, each through its own copy of the crate. Two
# copies would build and link, and the item size could differ at run time.
set -euo pipefail

TARGET="${TARGET:-riscv32imac-unknown-none-elf}"
SEAM_CRATES='bt-hci-transport|trouble-host|esp-[a-z0-9-]+|rs-matter[a-z0-9-]*|openthread(-sys|-driver|-driver-esp)?|embassy-executor(-timer-queue)?'

# "name vX.Y.Z" for every crate that appears in more than one version.
pairs=$(cargo tree --workspace --locked --target "$TARGET" -e normal -d --depth 0 --prefix none \
          | awk 'NF {print $1, $2}' | sort -u)
dups=$(echo "$pairs" | awk '{print $1}' | uniq -d | grep -E "^(${SEAM_CRATES})\$" || true)

if [ -n "$dups" ]; then
  echo "::error::Lockstep Set crates in more than one version: $(echo $dups)"
  # Show who pulls each version: a bare name is ambiguous to `cargo tree -i`.
  for d in $dups; do
    echo "$pairs" | awk -v d="$d" '$1 == d {sub(/^v/, "", $2); print $2}' | while read -r v; do
      cargo tree --workspace --locked --target "$TARGET" -e normal -i "$d@$v"
    done
  done
  exit 1
fi
echo "No Lockstep Set duplicates."
