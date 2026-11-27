#!/usr/bin/env bash
# Flash a Firmware and keep its serial log. The cargo runner for the ESP32-C6
# (.cargo/config.toml), so `cargo run` ends up here; works in Git Bash on
# Windows too.
#
#   cargo run --release [--features ...]     flash, then monitor
#   bash scripts/flash.sh --attach [ELF]     monitor the running Device again,
#                                            without resetting it
#
# The monitor output shows in the terminal as usual and is also saved, without
# colour codes, to target/serial/<UTC time>.log (the newest file is the latest
# run), so a log stays readable after the terminal has scrolled past it.
# espflash runs with --non-interactive because its output goes through a pipe;
# Ctrl+C still stops it. ESPFLASH_PORT picks the port when several are present.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
log_dir="$root/target/serial"
mkdir -p "$log_dir"
log="$log_dir/$(date -u +%Y-%m-%dT%H-%M-%SZ).log"

if [ "${1:-}" = "--attach" ]; then
  shift
  cmd=(espflash monitor --non-interactive --no-reset --chip esp32c6)
  [ $# -gt 0 ] && cmd+=(--elf "$1")
else
  [ $# -gt 0 ] || { echo "usage: $0 ELF [espflash args] | --attach [ELF]" >&2; exit 2; }
  cmd=(espflash flash --monitor --non-interactive --chip esp32c6 "$@")
fi

echo "Serial log: $log" >&2
"${cmd[@]}" 2>&1 | tee >(sed -u 's/\x1b\[[0-9;]*m//g' > "$log")
