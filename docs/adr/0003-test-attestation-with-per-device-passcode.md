# Permanent test attestation, but a unique Setup Passcode per Device

Devices permanently use Matter test attestation (test vendor ID 0xFFF1, the test DAC/PAI) and accept the controller's "uncertified device" warning, because real attestation requires CSA membership and certification, which makes no sense for personal Devices. Each Device still gets its own Setup Passcode instead of the shared test passcode 20202021, so that a nearby Device waiting for Commissioning cannot be claimed with a well-known code.
