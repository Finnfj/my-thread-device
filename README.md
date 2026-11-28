# my-thread-device

A Matter-over-Thread **On/Off Light** device running on the
[WeActStudio ESP32-C6-MINI](https://github.com/WeActStudio/WeActStudio.ESP32C6-MINI)
dev board, written in no\_std Rust with [Embassy](https://embassy.dev/).

The WS2812B RGB LED on **GPIO8** is the controlled endpoint. A Matter controller
(Google Home, Apple Home, Home Assistant, etc.) can turn it on and off over a
Thread network.

---

## How it works

| Phase | Transport |
|---|---|
| Commissioning | BLE (non-concurrent — BLE stops once Thread starts) |
| Operation | Thread → Matter UDP |

On first boot the device advertises over BLE. You scan the QR code / manual
pairing code printed to the serial monitor in your Matter controller app. After
commissioning finishes, BLE stops and the device joins the Thread network through
your Thread Border Router. All subsequent Matter traffic runs over Thread.

The Device keeps its Commissioning and its Thread network in the `matter-kv`
flash partition ([platform/src/persist.rs](platform/src/persist.rs)), so after a
reboot or a reflash it rejoins on its own; no new Commissioning is needed. The
light comes back on or off as it was, a few seconds after the last switch, as
rs-matter's On/Off cluster stores its state there too.

---

## Hardware

| Item | Detail |
|---|---|
| Board | WeActStudio ESP32-C6-MINI (ESP32-C6-FH4, 4 MB flash) |
| WS2812B LED pin | GPIO8 |
| Thread Border Router | Required — any Apple TV 4K, Google Nest Hub (2nd gen+), Home Assistant Yellow/Green, or a standalone OpenThread BR |

---

## Prerequisites

### Rust toolchain

The project uses **stable Rust** (`rust-toolchain.toml` pins the `stable` channel).
`rustup` picks it up automatically on first build; no manual steps are needed
beyond having rustup installed.

```
# Install rustup if you don't have it
# https://rustup.rs
```

The `riscv32imac-unknown-none-elf` target and the `rust-src` component are
declared in `rust-toolchain.toml` and installed automatically.

### C build tools (required for OpenThread)

OpenThread is a C library. The Rust crate ships **pre-compiled** static
libraries for `riscv32imac-unknown-none-elf`, but only for its default
features. This project also sets OpenThread's internal heap to 8 KiB
(rs-matter-embassy's `openthread-heap-int-8192`), so OpenThread is compiled
from source.

You need:

| Tool | Where to get it | Notes |
|---|---|---|
| **LLVM / Clang** | <https://releases.llvm.org/download.html> or `winget install LLVM.LLVM` | Add the `bin\` folder to `PATH` |
| **CMake ≥ 3.20** | <https://cmake.org/download/> or `winget install Kitware.CMake` | Add to `PATH` |
| **Ninja** | <https://ninja-build.org/> or `winget install Ninja-build.Ninja` | Add to `PATH` |

`CMAKE_GENERATOR=Ninja` is set in `.cargo/config.toml` so CMake uses Clang
instead of defaulting to the Visual Studio generator (which would use MSVC and
break the RISC-V cross-compile sysroot headers).

Verify all three are on your `PATH`:

```powershell
clang --version
cmake --version
ninja --version
```

### espflash

Used to flash and monitor the device.

```powershell
cargo install espflash
```

---

## Building

```powershell
cargo build
```

The first build compiles OpenThread from source (takes a few minutes). All
subsequent builds reuse the cached static library.

The firmware ELF is written to:

```
target\riscv32imac-unknown-none-elf\debug\on-off-light
```

---

## Flashing

Connect the board over USB, then:

```powershell
espflash flash target\riscv32imac-unknown-none-elf\debug\on-off-light --baud 1500000
```

Or use the Cargo runner shortcut from the workspace root (flashes **and** opens the monitor, through [scripts/flash.sh](scripts/flash.sh)):

```powershell
cargo run
```

The monitor output is also saved to `target/serial/<UTC time>.log`, so a full
log from boot is still there after the terminal has scrolled past it.

Run either from the repository root: espflash picks up `espflash.toml` there,
which makes it flash the repo's own partition table (`partitions.csv`, ADR
0008). Flashing only writes the bootloader, the partition table and the app,
so the `matter-kv` partition, and with it a commissioned Device's fabrics and
Thread dataset, survives a reflash of the same Firmware.

Persisted State is stamped with the Firmware ID (`FIRMWARE_ID` in the
Firmware's `main.rs`) and the Platform's Persisted State format version
(`FORMAT_VERSION` in [platform/src/persist.rs](platform/src/persist.rs)). If
either differs at boot, because another Firmware was flashed onto the Board or
an update bumped the format, the Device Factory Resets and has to be
commissioned again (ADR 0009). The serial log says so.

Erase `matter-kv` once on a Board that was last flashed without this
partition table, because the old app image sat where `matter-kv` is now. The
same flag also brings a Device back to uncommissioned until the Factory Reset
gesture exists:

```powershell
cargo run -- --erase-parts matter-kv
```

---

## Monitoring the serial output

To watch a running Device again without resetting it (also saved to `target/serial/`):

```powershell
bash scripts/flash.sh --attach target\riscv32imac-unknown-none-elf\debug\on-off-light
```

Or plain espflash, which resets the Device and saves nothing:

```powershell
espflash monitor --elf target\riscv32imac-unknown-none-elf\debug\on-off-light
```

On boot you will see something like:

```
I - Starting Matter Thread device...
I - QR Code URL: https://project-chip.github.io/connectedhomeip/qrcode.html?data=...
I - Manual pairing code: 3497-989-5191
```

The log level is set by `ESP_LOG` in `.cargo/config.toml`: `debug`, except for
the modules that would print the Thread network key or other Matter secrets.
See the comment there before changing it, and rebuild after a change.

---

## Commissioning

1. Open your Matter controller app (Google Home / Apple Home / Home Assistant).
2. Choose **Add device** → **Matter** → **Scan QR code**.
3. Point the camera at the QR code displayed in the serial monitor, or enter the
   manual pairing code.
4. The app will guide you through the rest. Ignore the "not certified" warning —
   the firmware uses test attestation credentials.
5. Once commissioned, use the app to turn the light on and off. The WS2812B LED
   on GPIO8 will respond in real time.

> **Home Assistant:** enable the **test-net DCL** option of the Matter Server
> add-on first. Its production trust policy rejects the test attestation
> outright, with no warning to accept: the phone commissions the device, then
> the hand-over to Home Assistant fails with a bare "An error occurred". The
> add-on log names the cause: "This device uses a test/development certificate".

> **Requires a Thread Border Router** on your network. The controller app will
> usually detect and configure it automatically if you already have a compatible
> smart home hub.

---

## On-device tests

Platform behaviour that needs the real chip (flash, later eFuse identity and
the Factory Reset gesture) is tested on a **Test Board** (ADR 0011), with
[embedded-test](https://github.com/probe-rs/embedded-test) and
[probe-rs](https://probe.rs/) over the Board's own USB-Serial-JTAG; no
external probe is needed. The tests are the Platform's unit tests, in
[platform/src/](platform/src/) under `#[cfg(test)]`.

> **The tests erase Persisted State.** A commissioned Device loses its
> Commissioning, so `cargo xtask hil` only runs on a Board you opted in.

Install probe-rs once:

```powershell
cargo install probe-rs-tools
```

### Opt a Board in

1. Plug the Board in and list the probes:

   ```powershell
   probe-rs list
   ```

   The Board shows up as `ESP JTAG -- 303a:1001:58:E6:C5:F9:7D:8C (EspJtag)`.
   Its serial number is the part after `303a:1001:` (the chip's MAC address).
2. Add that serial number as a line to `test-boards.txt` in the repository
   root (gitignored; one serial number per line, `#` starts a comment):

   ```
   # WeAct ESP32-C6-MINI on the bench
   58:E6:C5:F9:7D:8C
   ```

### Run the tests

From the repository root:

```powershell
cargo xtask hil
```

It builds the tests, flashes them with probe-rs to the opted-in Board only
(by its serial number, together with `partitions.csv`) and runs each test
after a reset. Arguments filter by test name, for example
`cargo xtask hil survives_reset`. With no opted-in probe connected, it refuses
and runs nothing. A plain `cargo test` builds the tests but refuses to flash
them, because its runner is espflash, which has no opt-in check.

### Afterwards

The Board is left with the test binary and the tests' Persisted State in
`matter-kv`, stamped with the tests' own Firmware ID. Flash the Firmware again
with `cargo run`: at its first boot it finds that stamp and Factory Resets.
Then commission it. If the Board is also your
Device, remove its line from `test-boards.txt` once you are done, so that a
later `cargo xtask hil` can't wipe it while it is commissioned.

---

## Project structure

```
.
├── .cargo/
│   └── config.toml        # Build target, runner, CMAKE_GENERATOR=Ninja
├── platform/              # Platform crate: chip + Matter-over-Thread stack bring-up
│   ├── build.rs           # Linker scripts for the on-device test binary
│   └── src/
│       ├── lib.rs
│       ├── persist.rs     # Persisted State in the matter-kv flash partition
│       └── test_support.rs # Set-up shared by the on-device tests
├── persist-stamp/         # Firmware ID + format version check (host-tested)
├── firmware/
│   └── on-off-light/      # Firmware: On/Off Light on a WS2812B LED
│       ├── build.rs       # Linker script wiring
│       └── src/main.rs    # Device Type, LED driver, entry point
├── xtask/                 # `cargo xtask hil`: on-device tests on a Test Board
├── docs/                  # architecture.md and ADRs
├── Cargo.toml             # Workspace, shared dependencies and git pins
├── espflash.toml          # Makes espflash flash partitions.csv
├── partitions.csv         # Partition table, with the matter-kv partition
├── GLOSSARY.md            # Domain vocabulary
└── rust-toolchain.toml    # Pins stable Rust + riscv32imac target
```

A Firmware describes its Device Types with rs-matter's own types and drives
its hardware; the Platform owns everything Devices have in common (see
[docs/architecture.md](docs/architecture.md)). `cargo build` / `cargo run` at
the root build and flash the On/Off Light.

### Key crates

| Crate | Role |
|---|---|
| [`esp-hal`](https://github.com/esp-rs/esp-hal) | Bare-metal ESP32-C6 HAL (RMT, GPIO, timers, RNG, …) |
| [`esp-rtos`](https://github.com/esp-rs/esp-hal) | Embassy runtime glue for ESP chips |
| [`esp-radio`](https://github.com/esp-rs/esp-hal) | IEEE 802.15.4 + BLE radio driver |
| [`rs-matter-embassy`](https://github.com/sysgrok/rs-matter-embassy) *(git)* | High-level Matter + Thread + BLE commissioning stack |
| [`openthread`](https://github.com/esp-rs/openthread) | Rust bindings for the OpenThread C library |
| [`openthread-driver-esp`](https://github.com/esp-rs/openthread) | The ESP32-C6 IEEE 802.15.4 radio under OpenThread |
| [`tinyrlibc`](https://github.com/rust-embedded-community/tinyrlibc) | C `str*` polyfills required by the pre-compiled OpenThread library |
| [`embassy-*`](https://embassy.dev/) | Async executor, synchronisation primitives, time |
| [`esp-backtrace`](https://github.com/esp-rs/esp-hal) | Panic handler and stack-trace printer |

---

## Known limitations / next steps

- **No Factory Reset gesture** — to make a Device uncommissioned again, erase
  `matter-kv` with `cargo run -- --erase-parts matter-kv` (see Flashing).
- **Test attestation** — the firmware uses development DAC/PAI certificates
  (`TEST_DEV_ATT`). It cannot be certified and controllers will warn about it.
- **No color control** — the WS2812B is driven as a simple on/off light (warm
  amber when on). Adding the `ColorControl` cluster is a natural extension.

---

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
