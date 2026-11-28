## Agent skills

### Issue tracker

Issues live in GitHub Issues on Finnfj/my-thread-device (via `gh`). See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role vocabulary: needs-triage, needs-info, ready-for-agent, ready-for-human, wontfix. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Dependency sources

`bash scripts/dep-src.sh <crate>` prints the source folder of the version Cargo.lock pins; search there, not across `~/.cargo/registry`. `<crate>@<version>` fetches one that isn't in Cargo.lock, such as `probe-rs-tools@0.31.0`. esp-hal's clock tree and per-chip peripheral code live in `esp-metadata-generated`.

## Device logs

`cargo run` saves the Device's serial log to `target/serial/` (newest file = latest run). `bash scripts/flash.sh --attach` reattaches to a running Device without resetting it.

## On-device tests

`cargo xtask hil` runs the Platform's unit tests on a Test Board; set-up and probe-rs limits are in `platform/src/test_support.rs` and ADR 0011. The first build of the test binary takes minutes: run it in the background.

## Before opening a PR

Run the `code-review` skill against `origin/master`; this repo's review rules are in `CODING_STANDARDS.md`.
