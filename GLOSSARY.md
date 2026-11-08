# my-thread-device

Self-built, personal smart-home devices that speak Matter over Thread, written in no_std Rust. This glossary fixes the language used across the platform and every device built on it.

## Devices and hardware

**Device**:
One self-built physical thing running one Firmware; it is exactly one Matter node on the network.
_Avoid_: Node (except when speaking strictly about Matter protocol), gadget, thing

**Device Type**:
The Matter-defined role of one endpoint on a Device, such as On/Off Light or Contact Sensor. A Device can expose several.
_Avoid_: Device (for the role), profile

**Board**:
The hardware module a Device is built on, such as the WeAct ESP32-C6-MINI.
_Avoid_: Device (for the hardware), chip, devkit

**Firmware**:
The program for one kind of Device. Several identical Devices can run the same Firmware.
_Avoid_: App, image, binary (when meaning the kind of Device)

**Test Board**:
A Board set aside for automated hardware tests, whose Persisted State may be wiped at any time; never a Device.
_Avoid_: Dev board, test device

**Platform**:
The shared foundation every Firmware is built on: Matter stack setup, Persisted State, Device Identity, and Power Mode handling.
_Avoid_: Framework, core, common, SDK

## Power

**Power Mode**:
Whether a Device is Always-On or Sleepy; chosen per Device, not globally.

**Always-On Device**:
A Device whose radio is always listening, typically mains-powered.
_Avoid_: Powered device, router (a separate Thread role)

**Sleepy Device**:
A battery Device that stays attached to the Thread network but turns its radio off between polls to its parent, as a Thread sleepy end device / Matter ICD.
_Avoid_: Deep-sleep device, low-power device, battery device

## Lifecycle

**Commissioning**:
Adding a Device to a controller's Matter fabric, started by scanning its Pairing Code.
_Avoid_: Pairing, onboarding, adding

**Decommissioning**:
Removing the last fabric from a Device so that no controller owns it any more.
_Avoid_: Unpairing, removing

**Factory Reset**:
Wiping all Persisted State on a Device, returning it to its freshly flashed state except for its Device Identity.
_Avoid_: Reset, wipe, erase

## Persisted State

**Persisted State**:
Everything a Device remembers across reboots; made of Commissioning State, Network State, and Application State.
_Avoid_: Settings, config, NVS, storage

**Commissioning State**:
The Persisted State that makes a Device belong to its controllers: fabrics, access control, and operational certificates. Cleared by Decommissioning and by Factory Reset.

**Network State**:
The Persisted State that lets a Device rejoin its Thread network without Commissioning again: the Thread dataset. Cleared by Decommissioning and by Factory Reset.
_Avoid_: Credentials, Wi-Fi settings

**Application State**:
The Persisted State owned by a Firmware's own Device Types, such as whether a light is on or its start-up behaviour. Survives Decommissioning; cleared only by Factory Reset.
_Avoid_: User data, cluster state

## Identity

**Device Identity**:
The values that name a Device for its whole lifetime: its Thread EUI-64, Discriminator, and Setup Passcode. Derived from the chip, never randomly regenerated, and not part of Persisted State.
_Avoid_: Credentials, serial

**Pairing Code**:
The QR code or manual code that encodes a Device's Discriminator and Setup Passcode for Commissioning.
_Avoid_: QR code (for the concept), setup code

**Setup Passcode**:
The per-Device secret proven during Commissioning; unique to each Device.
_Avoid_: PIN, password

**Discriminator**:
The 12-bit number that lets a controller tell apart Devices waiting for Commissioning.

**Build Secret**:
The private key, held outside the repository, from which every Device's Setup Passcode and Discriminator are derived.
_Avoid_: Salt, seed, master key

## Development

**Lockstep Set**:
The dependencies whose types are shared with `esp-hal`, `esp-radio`, `esp-rtos` or `rs-matter-embassy`, so that they can only be upgraded together, never one at a time.
_Avoid_: esp family, pinned deps, the stack
