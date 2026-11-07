# Three test tiers, with on-device tests run locally on a dedicated Test Board

Testing has three tiers:

1. Host tests (`cargo test`) for pure logic.
2. On-device tests with `embedded-test` and probe-rs over the ESP32-C6's built-in USB-Serial-JTAG, for Platform behaviour that can't be faked on a host: flash round-trips across reboots, Firmware ID mismatch wiping, eFuse-derived identity, and the Factory Reset gesture.
3. End-to-end Matter tests: commission over BLE through a Thread Border Router, toggle, reboot, and check the Device returns without re-Commissioning.

On-device tests erase Persisted State, so they run only on a dedicated Test Board that opts in through a gitignored config (probe serial number), never on a commissioned Device. They run locally through `cargo xtask hil`, not in CI, because GitHub-hosted runners have no hardware; CI runs build, clippy, and host tests.

## Consequences

- Tier 3 is a goal, not set up yet: it needs a Linux test rig with BLE, a Thread Border Router, and `chip-tool` (for example a Raspberry Pi with an RCP dongle), which the Windows dev machine can't easily provide. A self-hosted CI runner becomes worth it once that rig exists.
- Not yet verified: that the WeAct ESP32-C6-MINI's USB-C is wired to the chip's native USB (check with `probe-rs list`), and that `embedded-test` works with this project's esp-rtos/Embassy versions.
