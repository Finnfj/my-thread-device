# Cargo workspace: one Platform crate, one crate per Firmware

The repository is a Cargo workspace with a shared Platform crate (Matter stack setup, Persisted State, Device Identity, Power Mode) and one binary crate per Firmware, which only describes its Device Types and drives its hardware. We set this boundary now, while there is only one Firmware (the On/Off Light), because the persistence work is almost entirely shared code, and putting it in a single binary first would mean pulling it apart again for the second Device.

## Consequences

- Layout: `platform/` (the Platform crate), `identity/` (a small no_std crate with the Setup Passcode, Discriminator and EUI-64 derivation, shared by the Platform and the host tool so they can never disagree about a Pairing Code), `firmware/<name>/` (one per Firmware, starting with `firmware/on-off-light/`), and `xtask/` (host commands such as `pairing-code <mac>`).
- Firmware describes its Device Types with rs-matter's cluster types and handlers directly. The Platform owns only the lifecycle (start-up, Persisted State, identity, Factory Reset) and does not wrap the Matter data model, so rs-matter API changes only need chasing in one place.
