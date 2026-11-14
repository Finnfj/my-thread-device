# What Persisted State covers, and what it deliberately doesn't

Persisted State covers Commissioning State (stored by the Matter stack), Network State, and Application State (each Firmware stores its own, since the stack does not persist app clusters). Device Identity is not stored at all: it is derived from the chip's factory MAC so it stays the same across Factory Reset and full flash erase, and the Pairing Code printed for a Device stays valid for its whole life.

Network State is only the Thread dataset, which the Matter stack already stores. OpenThread's own settings (frame counters, parent and child info) stay in RAM, as rs-matter-embassy provides (it persists only the SRP key; checked in rs-matter-embassy @2a806b0), so after a reboot a Device re-attaches to Thread from scratch. That is slower but works, and storing those settings would mean replacing rs-matter-embassy's OpenThread settings layer.

Application State outlives Decommissioning (a light remembers its start-up behaviour when moved to another controller) but is cleared by Factory Reset. Factory Reset is triggered by holding the Board's BOOT button for 10 s at any time while the Device is running, not at start-up: on the ESP32-C6 the BOOT button is the GPIO9 strapping pin, and holding it at reset enters ROM download mode. The Platform owns the gesture; the Firmware only gets a hook to show feedback (for example, the light blinks) before the wipe. Commissioning and Network State are also cleared automatically when the last fabric is removed.

## Consequences

- The random Discriminator and EUI-64 generated on every boot (copied from the upstream example) must be replaced before persistence is turned on. Otherwise the Pairing Code changes on every boot, and the stored SRP key would be reused under a new SRP host name, which is the EUI-64.
- rs-matter does not do the automatic clearing on its own: `RemoveFabric` deletes only that fabric's record, and the Thread dataset stays stored (checked in rs-matter 0.4.1, `dm/clusters/noc.rs`). The Platform has to notice that the last fabric is gone and clear Network State itself.
- After clearing Network State on Decommissioning, the Platform reboots the Device, so it comes back exactly like a freshly flashed one (uncommissioned, advertising over BLE) instead of tearing down Thread while running.
