# Coding standards

Read during review (the `code-review` skill), not while implementing. Each rule
here is a judgement call; anything a script can check belongs in `scripts/` and
CI instead (`.github/workflows/ci.yml`).

## Docs follow behaviour

A PR that changes what the Platform or a Firmware does updates every statement
of the old behaviour in `README.md`, `docs/` and the ADRs, in the same PR.
Search for each name the diff removes or replaces (types, features, flags,
partition names) and for prose about the behaviour itself ("not persisted",
"lost on reboot").

_Why:_ #19 switched to the `matter-kv` store and left three README passages
saying state isn't persisted and naming the removed `DummyKvBlobStore`.

ADR wording and markers follow `docs/agents/domain.md`.
