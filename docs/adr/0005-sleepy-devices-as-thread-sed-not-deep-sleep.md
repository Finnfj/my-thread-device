# Sleepy Devices are Thread sleepy end devices, not deep-sleep reboots

A Sleepy Device stays attached to its Thread parent and turns its radio off between polls (a Thread SED, announced as a Matter ICD). By default it keeps RAM and Matter sessions and light-sleeps between polls. We rejected ESP32 deep sleep as a reboot from scratch: each wake would need a full Thread attach and a new Matter session, and without ICD support controllers would show the Device as offline between wakes.

An event-driven sensor, such as a reed contact or a PIR, that runs as a long-idle-time (LIT) ICD may deep sleep between check-ins instead, as long as each wake resumes rather than starts over. OpenThread's settings (parent info, frame counters) are persisted, so it rejoins its parent with one Child Update Request within the child timeout. CASE resumption and persistent subscriptions let controllers carry on without setting up new sessions. To its parent and its controllers it is still a Sleepy Device.

Thread SSED (CSL) is not an option on the ESP32-C6: openthread is built without a CSL receiver, and the ESP radio has no timed receive (checked in openthread @ac97457).

Amended on 2026-11-14 after [the battery sensor research](../research/battery-sensor-esphome-vs-no-std.md), checked against master @ 8c92940.

## Consequences

- What blocks Sleepy Devices now is ESP 802.15.4 radio sleep. esp-radio has no way to turn the receiver off (esp-hal PR #6429 is open; checked in esp-radio 1.0.0-beta.1), and openthread's `EspRadio::set_sleep` is a TODO (checked in openthread @ac97457), so a SED keeps its receiver on at about 74 mA. The other pieces exist at the pinned versions but are not wired up:
  - rs-matter has the ICD Management cluster and Check-In (checked in rs-matter 0.4.1). The active/idle control loop is only on git, after 0.4.1.
  - esp-rtos has experimental automatic light sleep (checked in esp-rtos 0.4.0).
  - The openthread binding has link mode and poll period (checked in openthread @ac97457), but rs-matter-embassy neither uses nor exposes them (checked in rs-matter-embassy @2a806b0).

  Sleepy Devices are a goal, not a feature, until radio sleep exists here or upstream.
- Persistence does not depend on this and goes first.
- The deep-sleep variant needs OpenThread's settings persisted, which ADR-0006 currently leaves in RAM, so ADR-0006 must be revisited before building one. It also needs rs-matter's `case-resumption` and `persistent-subscriptions` features turned on.
- Only GPIO0–7 can wake the ESP32-C6 from deep sleep, and only on a level, not an edge. A deep-sleeping sensor's wake pin must be one of those and be re-armed to the opposite level before each sleep. The BOOT button (GPIO9) cannot wake it.
- ADR-0010 keeps the commissioning window open until the Device is commissioned, which means advertising over BLE indefinitely on battery. Sleepy Devices need a bounded window or a button trigger. That is not decided yet.
