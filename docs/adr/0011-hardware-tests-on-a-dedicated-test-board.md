# Three test tiers, with on-device tests run locally on a dedicated Test Board

Testing has three tiers:

1. Host tests (`cargo test`) for pure logic.
2. On-device tests with `embedded-test` and probe-rs over the ESP32-C6's built-in USB-Serial-JTAG, for Platform behaviour that can't be faked on a host: flash round-trips across reboots, Firmware ID mismatch wiping, eFuse-derived identity, and the Factory Reset gesture.
3. End-to-end Matter tests: commission over BLE through a Thread Border Router, toggle, reboot, and check the Device returns without re-Commissioning.

On-device tests erase Persisted State, so they run only on a dedicated Test Board that opts in through a gitignored config (probe serial number), never on a commissioned Device. They run locally through `cargo xtask hil`, not in CI, because GitHub-hosted runners have no hardware; CI runs build, clippy, and host tests.

## Consequences

- Tier 3 is a goal, not set up yet: it needs a Linux test rig with BLE, a Thread Border Router, and `chip-tool` (for example a Raspberry Pi with an RCP dongle), which the Windows dev machine can't easily provide. A self-hosted CI runner becomes worth it once that rig exists.
- The WeAct ESP32-C6-MINI's USB-C is wired to the chip's native USB-Serial-JTAG (espflash sees Espressif's VID `303A`, no USB-UART bridge), so probe-rs should need no external probe. That port drops while the chip light-sleeps; this and the bench's other limits are in [docs/test-board.md](../test-board.md).
- probe-rs sees that port as an `EspJtag` probe whose serial number is the chip's MAC address (`303a:1001:58:E6:C5:F9:7D:8C` on the bench, probe-rs 0.31.0), so the opt-in list (`test-boards.txt`) names Boards by that serial and `cargo xtask hil` hands it to `probe-rs run --probe`.
- `embedded-test` builds and links with this project's stack. The tests are the Platform's unit tests; async ones run on esp-rtos's executor through embedded-test's `embassy-010` and `external-executor` features (checked in embedded-test 0.7.3), the set-up esp-hal's own hardware tests use (checked in esp-hal 1.2.2), with esp-rtos's `embassy::Executor` (checked in esp-rtos 0.4.0) on Embassy's executor (checked in embassy-executor 0.10.0). That makes embedded-test part of the Lockstep Set. Running them on the Board is not verified yet.
- probe-rs resets the chip before every test, runs the tests in the order of the test binary's symbol table, and aborts the run if a test resets the chip itself (probe-rs 0.31.0). So "survives a reset" is two tests, one storing and one loading, and the second fails if it runs without the first.
- probe-rs needs `--idf-partition-table partitions.csv`, otherwise it flashes the default table and the Platform finds no `matter-kv`. It writes only the bootloader, the partition table and the test binary; the tests erase `matter-kv` themselves.
