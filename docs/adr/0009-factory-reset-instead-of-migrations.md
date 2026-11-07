# A Firmware ID and format version mismatch triggers a Factory Reset, not a migration

The Platform stores a Firmware ID and a Persisted State format version. If either doesn't match at boot (a different Firmware was flashed onto the Board, or an update changed a stored format, ours or rs-matter's), the Device does a Factory Reset instead of migrating. For a small personal fleet, re-Commissioning after a breaking update is cheaper than writing and testing migrations, and it stops one Firmware from misreading another's Application State.
