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

State is **not persisted** across reboots (a `DummyKvBlobStore` is used). If the
device restarts you need to remove it from your controller and re-commission.

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

The project uses **stable Rust** (`rust-toolchain.toml` pins the exact channel).
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
libraries for `riscv32imac-unknown-none-elf` when using the published crates.io
versions, but this project pins to the git HEAD of
[esp-rs/openthread](https://github.com/esp-rs/openthread) (for a bundled-MbedTLS
fix), which must be compiled from source.

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

Or use the Cargo runner shortcut (flashes **and** opens the monitor):

```powershell
cargo run
```

---

## Monitoring the serial output

```powershell
espflash monitor --elf target\riscv32imac-unknown-none-elf\debug\on-off-light
```

On boot you will see something like:

```
I - Starting Matter Thread device...
I - QR Code URL: https://project-chip.github.io/connectedhomeip/qrcode.html?data=...
I - Manual pairing code: 3497-989-5191
```

Set `ESP_LOG=debug` in your environment for more verbose output.

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

> **Requires a Thread Border Router** on your network. The controller app will
> usually detect and configure it automatically if you already have a compatible
> smart home hub.

---

## Project structure

```
.
├── .cargo/
│   └── config.toml        # Build target, runner, CMAKE_GENERATOR=Ninja
├── platform/              # Platform crate: chip + Matter-over-Thread stack bring-up
│   └── src/lib.rs
├── firmware/
│   └── on-off-light/      # Firmware: On/Off Light on a WS2812B LED
│       ├── build.rs       # Linker script wiring
│       └── src/main.rs    # Device Type, LED driver, entry point
├── docs/                  # architecture.md and ADRs
├── Cargo.toml             # Workspace, shared dependencies and git pins
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
| [`openthread`](https://github.com/esp-rs/openthread) *(git HEAD)* | Rust bindings for the OpenThread C library |
| [`tinyrlibc`](https://github.com/rust-embedded-community/tinyrlibc) | C `str*` polyfills required by the pre-compiled OpenThread library |
| [`embassy-*`](https://embassy.dev/) | Async executor, synchronisation primitives, time |
| [`esp-backtrace`](https://github.com/esp-rs/esp-hal) | Panic handler and stack-trace printer |

---

## Known limitations / next steps

- **No persistence** — Matter fabric and network credentials are lost on reboot.
  Replace `DummyKvBlobStore` with `SeqMapKvBlobStore` backed by `esp-storage` to
  fix this.
- **Test attestation** — the firmware uses development DAC/PAI certificates
  (`TEST_DEV_ATT`). It cannot be certified and controllers will warn about it.
- **No color control** — the WS2812B is driven as a simple on/off light (warm
  amber when on). Adding the `ColorControl` cluster is a natural extension.
- **Single reboot = decommission** — until persistence is added, every reboot
  requires removing and re-adding the device in your controller app.
