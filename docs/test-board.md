# Test Board and bench

What the hardware on the bench can and can't do, as found during the light-sleep
measurements (closed spike PR #16). Plan on-device work and measurement
checklists around this, not around a generic lab.

## Board

- WeActStudio ESP32-C6-MINI. Its USB-C goes straight to the chip's native
  USB-Serial-JTAG; there is no USB-UART bridge. espflash sees it as Espressif's
  USB ID (VID `303A`), on COM5 on the dev machine.
- No separate USB-UART adapter is on hand, so the only log is the
  USB-Serial-JTAG one.

## On-device tests

- probe-rs (0.31.0) lists the Board as an `EspJtag` probe,
  `303a:1001:58:E6:C5:F9:7D:8C`: the serial number is the chip's MAC address.
  That serial is what `test-boards.txt` opts in (README, On-device tests).
- probe-rs and espflash use the same USB port. Close a running serial monitor
  before `cargo xtask hil`.
- A run leaves the test binary on the Board, and it only works under probe-rs:
  flash the Firmware back with `cargo run` afterwards.

## USB in light sleep

- The USB-Serial-JTAG port disappears as soon as the chip enters light sleep:
  the log stops mid-run and espflash exits with os error 22.
- With the port gone, espflash can't reset or reflash the Board. To get it
  back, hold BOOT while plugging the cable in (download mode), then flash a
  build without light sleep.
- So a build that light-sleeps gives no log once it sleeps. Log what matters
  before the first sleep, or answer the question with the power meter.

## Power measurement

- The only meter is an inline USB-C meter with mAh and mWh counters and a slow
  live reading. There is no PPK2, shunt or scope.
- It measures the whole Board at 5 V (regulator, WS2812 idle draw, chip), so
  it suits A/B comparisons, not absolute chip current.
- Read the counters, not the live value. The mAh counter moves in whole steps
  (1 mAh is about 5 mA over 12 minutes), so compare mWh. Repeated runs of the
  same build differ by about 5%; smaller differences are noise.
- Two setups:
  - **PC → meter → Board** gives the log.
  - **Charger → meter → Board** has no USB data, so the chip's USB stays idle,
    but there is no log. The charger's voltage is lower than the PC port's, so
    compare runs within one setup only.
