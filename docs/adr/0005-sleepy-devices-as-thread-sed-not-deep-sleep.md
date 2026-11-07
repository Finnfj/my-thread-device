# Sleepy Devices are Thread sleepy end devices, not deep-sleep reboots

A Sleepy Device stays attached to its Thread parent and turns its radio off between polls (Thread SED/SSED, announced as a Matter ICD), keeping RAM and Matter sessions. We rejected ESP32 deep sleep, where every wake is effectively a reboot: each wake would need a full Thread re-attach and a new Matter session, and without ICD support controllers would show the Device as offline between wakes.

## Consequences

- As of 2026-11 this is blocked upstream at every layer: rs-matter has no ICD Management cluster or check-in protocol, rs-matter-embassy doesn't expose link mode or poll period, OpenThread/esp-radio radio sleep is a TODO, and esp-rtos has no automatic light sleep. Sleepy Devices are a goal, not a feature, until that work is done here or upstream.
- Persistence does not depend on this and goes first.
