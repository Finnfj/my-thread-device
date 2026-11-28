#!/usr/bin/env bash
# Tests scripts/flash.sh without a Board: it must refuse on-device test
# binaries (what `cargo test` hands its runner) and pass Firmware ELFs on to
# espflash. Uses FLASH_DRY_RUN, so nothing is flashed. Run by CI; works in Git
# Bash on Windows too.
set -euo pipefail

cd "$(dirname "$0")/.."
fail=0

expect() { # expect refuse|flash ELF
  local want=$1 elf=$2 out status=0
  out=$(FLASH_DRY_RUN=1 bash scripts/flash.sh "$elf" 2>&1) || status=$?
  case "$want:$status" in
    refuse:2) [[ $out == *"on-device test binary"* ]] && return ;;
    flash:0) [[ $out == "espflash flash "*"$elf" ]] && return ;;
  esac
  echo "::error::flash.sh $elf: expected $want, got exit $status: $out"
  fail=1
}

expect refuse 'target/riscv32imac-unknown-none-elf/debug/deps/platform-1a2b3c'
expect refuse 'C:\A\target\riscv32imac-unknown-none-elf\debug\deps\platform-1a2b3c'
expect flash 'target/riscv32imac-unknown-none-elf/debug/on-off-light'
expect flash 'C:\A\target\riscv32imac-unknown-none-elf\release\on-off-light'

[ "$fail" -eq 0 ] && echo "flash.sh guard: test binaries refused, Firmware passed on."
exit "$fail"
