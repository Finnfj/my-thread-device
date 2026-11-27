## Agent skills

### Issue tracker

Issues live in GitHub Issues on Finnfj/my-thread-device (via `gh`). See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role vocabulary: needs-triage, needs-info, ready-for-agent, ready-for-human, wontfix. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Device logs

`cargo run` saves the Device's serial log to `target/serial/` (newest file = latest run). `bash scripts/flash.sh --attach` reattaches to a running Device without resetting it.
