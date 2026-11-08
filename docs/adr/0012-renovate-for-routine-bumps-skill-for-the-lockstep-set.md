# Dependency updates: Renovate for routine bumps, a skill for the Lockstep Set

Renovate (Mend Community Cloud) opens one weekly PR for routine bumps and one for the lockfile refresh. Both are merged by hand, because CI can't run the firmware (see ADR-0011). The Lockstep Set (see GLOSSARY.md) only appears on Renovate's Dependency Dashboard, with approval required. It is upgraded as a whole by `/upgrade-lockstep-set`, a skill started by hand, which:

- checks each pin's `Unpin when:` comment in `Cargo.toml`;
- may adopt a pre-release when that is what lifts a pin;
- finishes with a PR that has CI green and a checklist of on-device checks only a human can do.

CI's duplicate guard is the safety net: Cargo happily resolves two semver-incompatible versions of a crate such as `bt-hci`, and a bot can't see that.

The list of Lockstep Set crates lives in `renovate.json`, the one place a machine reads it; this ADR and the glossary carry only the membership rule.

## Considered options

- **Dependabot:** it can't update dependencies pinned to a git `rev`, has no whole-lockfile refresh, and is expected to break on a Windows-host `rust-toolchain.toml` pin.
- **Bot for everything:** every Lockstep Set PR would be red by design, for example `esp-hal` bumped without the matching `esp-radio`.
- **Skill for everything:** an agent run every week for work a free bot already does.

Research: `docs/research/dependency-update-tooling.md`.
