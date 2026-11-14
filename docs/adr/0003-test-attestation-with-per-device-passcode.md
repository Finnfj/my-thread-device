# Permanent test attestation, but a unique Setup Passcode per Device

Devices permanently use Matter test attestation (test vendor ID 0xFFF1, the test DAC/PAI) and accept the controller's "uncertified device" warning, because real attestation requires CSA membership and certification, which makes no sense for personal Devices. Each Device still gets its own Setup Passcode instead of the shared test passcode 20202021, so that a nearby Device waiting for Commissioning cannot be claimed with a well-known code.

## Consequences

- Not every controller shows a warning to accept. Home Assistant's Matter Server rejects test attestation by policy until its **test-net DCL** option is enabled, so that switch is part of setting up Home Assistant (README, Commissioning).
