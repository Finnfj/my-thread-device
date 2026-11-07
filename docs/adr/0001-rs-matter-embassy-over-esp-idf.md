# Pure-Rust Matter stack (rs-matter-embassy) instead of ESP-IDF + connectedhomeip

Every Firmware is no_std Rust on Embassy, using rs-matter-embassy for Matter, OpenThread (esp-rs bindings) for Thread, and esp-hal / esp-radio for the hardware. We picked this over Espressif's ESP-IDF with the C++ connectedhomeip SDK (the "obvious" path, with a certified, feature-complete stack) because we want one async Rust codebase with no C++ toolchain or FreeRTOS, and the Devices are personal, so certification doesn't matter.

## Consequences

- The stack is young and only on git: dependencies are pinned to specific revisions, and upgrades need care (see the comments in `Cargo.toml`).
- Features we need are missing upstream and become our work or our wait: Matter ICD, Thread sleepy end device configuration, radio sleep, routing (OpenThread is built MTD-only), and persistence of OpenThread's own settings.
