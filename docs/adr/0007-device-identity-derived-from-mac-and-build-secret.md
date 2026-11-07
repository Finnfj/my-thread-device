# Device Identity is derived from the factory MAC and a Build Secret

The Setup Passcode and Discriminator are computed as HMAC(Build Secret, factory MAC); passcodes the Matter spec forbids (00000000, 11111111, 12345678, …) are skipped deterministically. The Thread EUI-64 comes straight from the MAC, following ESP-IDF's 802.15.4 convention. A host-side command computes the same Pairing Code from a MAC, so a label printed once stays valid for the Device's lifetime. We picked derivation over storing random values in flash (lost on full erase, which would invalidate printed labels) and over deriving from the MAC alone (the MAC is broadcast, so anyone nearby could compute the passcode).

## Consequences

- The Build Secret is kept out of the repo (gitignored file or environment variable), and a build without it fails rather than falling back to the test passcode.
- Changing the Build Secret or the derivation changes every Device's Pairing Code. Commissioned Devices keep working, but printed labels become wrong.
- The Build Secret is compiled into every Firmware image. Anyone with physical access to one Device can read its flash and compute the Setup Passcode of every other Device. We accept this: a passcode only matters while a Device is waiting for Commissioning, and an attacker with physical access going after your not-yet-commissioned Devices is not a realistic threat for a personal fleet. The rejected alternative was writing each passcode into a factory partition at provisioning time, so Devices never hold the secret.
