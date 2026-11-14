# ESP32-C6 Board family, with BLE used only for Commissioning

All Boards are ESP32-C6 based (802.15.4 + BLE on one chip). BLE is used only to carry Commissioning and is shut down once Thread starts; Thread and BLE never run at the same time. The ESP32-C6 is the chip rs-matter-embassy and esp-radio support best for Thread. Running the radios one after another avoided coexistence, which esp-radio's 802.15.4 driver did not support when this was decided. It now documents coexistence with BLE as supported (checked in esp-radio 1.0.0-beta.1), so running one radio at a time is now a choice rather than a constraint.

## Consequences

- A Device that is already commissioned cannot be re-commissioned over BLE without a Decommissioning or Factory Reset.
- Other 802.15.4 chips (ESP32-H2, nRF52840) are out of scope until there is a reason; the Platform does not abstract over chip vendors.
