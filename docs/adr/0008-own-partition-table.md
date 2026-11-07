# Own partition table with a dedicated Persisted State partition

The repo checks in its own partition table with a dedicated data partition for Persisted State (64 KiB, labelled `matter-kv`) instead of relying on espflash's default `nvs` partition (24 KiB, layout not pinned by us). Changing the layout on deployed Devices loses their Persisted State, so the layout has to be explicit and versioned with the code. Space is left for OTA slots, but none are created yet.
