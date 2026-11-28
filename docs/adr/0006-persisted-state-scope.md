# What Persisted State covers, and what it deliberately doesn't

Persisted State covers Commissioning State (stored by the Matter stack), Network State, and Application State (each Firmware's own, under keys the Platform sets aside for it; rs-matter's On/Off cluster handler stores its `OnOff` and `StartUpOnOff` attributes itself, under a key the Firmware picks, checked in rs-matter 0.5.0). Device Identity is not stored at all: it is derived from the chip's factory MAC so it stays the same across Factory Reset and full flash erase, and the Pairing Code printed for a Device stays valid for its whole life.

Network State is the Thread dataset, which the Matter stack stores, plus the OpenThread settings rs-matter-embassy persists: the active dataset, network info (with the frame counters), parent info, the SLAAC secret, and the SRP key and server (checked in rs-matter-embassy @d9456b8). So after a reboot a Device re-attaches to its old parent with one Child Update Request while its child entry is still valid, instead of attaching from scratch. The rest of OpenThread's settings stay in RAM.

Amended on 2026-11-28 for the Lockstep Set upgrade to rs-matter 0.5 and rs-matter-embassy @d9456b8, which turned on the persistence of OpenThread's settings and of the On/Off cluster's attributes. Before that, OpenThread's settings stayed in RAM apart from the SRP key.

Application State outlives Decommissioning (a light remembers its start-up behaviour when moved to another controller) but is cleared by Factory Reset. Factory Reset is triggered by holding the Board's BOOT button for 10 s at any time while the Device is running, not at start-up: on the ESP32-C6 the BOOT button is the GPIO9 strapping pin, and holding it at reset enters ROM download mode. The Platform owns the gesture; the Firmware only gets a hook to show feedback (for example, the light blinks) before the wipe. Commissioning and Network State are also cleared automatically when the last fabric is removed.

## Consequences

- The random Discriminator generated on every boot (copied from the upstream example) must be replaced, or the Pairing Code changes on every boot. The random EUI-64 no longer clashes with the stored SRP key: the SRP host name is the Thread extended address, which OpenThread keeps in its persisted settings together with that key (checked in rs-matter-embassy @d9456b8).
- rs-matter does not do the automatic clearing on its own: `RemoveFabric` deletes only that fabric's record, and the Thread dataset stays stored (checked in rs-matter 0.5.0, `remove_fabric` in rs-matter-node). The Platform has to notice that the last fabric is gone and clear Network State itself.
- After clearing Network State on Decommissioning, the Platform reboots the Device, so it comes back exactly like a freshly flashed one (uncommissioned, advertising over BLE) instead of tearing down Thread while running.
