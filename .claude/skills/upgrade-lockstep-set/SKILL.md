---
name: upgrade-lockstep-set
description: Upgrade every Lockstep Set crate together in one PR, with CI green and an on-device checklist for the user.
disable-model-invocation: true
---

# Upgrade the Lockstep Set

The **Lockstep Set** (`GLOSSARY.md`, policy in ADR-0012) has two parts:

- the crates matched by the `"Lockstep Set"` rule's `matchDepNames` in `renovate.json`, which is the only list;
- the `[patch.crates-io]` crates in `Cargo.toml`, which Renovate never reads.

A **seam** is a crate whose types cross between two members, such as `bt-hci-transport` between esp-radio and trouble-host. The seams are listed in `SEAM_CRATES` in `scripts/lockstep-guard.sh`.

Tools used throughout:

- `python -I .claude/skills/upgrade-lockstep-set/upstream_reqs.py crate NAME [VERSION]` prints the recent versions (yanked ones marked), or one version's seam dependencies.
- `python -I .claude/skills/upgrade-lockstep-set/upstream_reqs.py git URL --manifest PATH OLD..HEAD` prints, for each commit, the seam dependencies plus the `[patch.crates-io]` entries active in upstream's root. It bare-clones into `--cache` (a temp folder by default) and prints the clone's path as `# clone:`. Later steps run `git -C <clone> ...` in that same clone.
- `cargo tree --workspace --target riscv32imac-unknown-none-elf -e normal -i NAME@VERSION` shows who pulls a crate.
- `bash scripts/lockstep-guard.sh` is the same duplicate guard CI runs.
- `bash scripts/adr-version-guard.sh` lists every ADR claim marked `(checked in <crate> <version>)` that no longer matches `Cargo.lock`.

## 1. Survey

Build a **requirement table** before editing anything:

- every crates.io member: its latest non-yanked version and that version's seam dependencies;
- every git pin (the `rs-matter-embassy` rev, the `openthread` patch rev): each commit from the current rev to HEAD;
- every `Unpin when:` and `Drop the patch when:` line in `Cargo.toml`, checked against the table.

Treat every claim about upstream as a hypothesis until the table confirms it, whether it comes from a brief, a research doc or a pin comment. In the first run, "esp-radio beta.1 + bt-hci 0.9 lifts all pins" came from an unverified research side note; that pairing never existed upstream.

Picking targets:

- **Pre-releases** only where one lifts a pin (ADR-0012). Never pick a yanked version.
- **Git commits**: upstream's root `[patch.crates-io]` doesn't apply to us. A commit whose root patches a crate we don't already patch would add a member to the Lockstep Set, so prefer the newest commit that avoids that. If no such commit exists, say so in the PR and update `renovate.json`'s rule.
- **Duplicates**: when a requirement can't be met with a single version, look in the crate source (`~/.cargo/registry/src/*/<crate>-<version>/`) for which trait or type actually crosses. The seam can move: in the first run it moved from `bt-hci` to `bt-hci-transport`, which made `bt-hci` 0.9 and 0.10 side by side harmless.

Done when: every member has one target version or commit, the seam requirements are satisfiable together, and every pin condition is marked met or not met, with the table row that shows it.

## 2. Bump

Branch from `master`. Edit `Cargo.toml`, writing git revs as full SHAs.

Resolve the lockfile with any `cargo tree` call, not `cargo update`, so only what the bump needs moves; lockfile refreshes are Renovate's job. Compare `Cargo.lock` package versions before and after: every change must trace back to a bumped manifest.

Rewrite every pin comment you touched:

- why it is pinned now;
- an `Unpin when:` (or `Drop the patch when:`) condition that one lookup can check. Examples: "an esp-radio release on crates.io depends on X", "rs-matter-embassy's root Cargo.toml no longer patches rs-matter".

Delete a pin and its comment once its condition is met.

Done when: `Cargo.toml` names every target from step 1, and every pin comment states a current reason plus a one-lookup condition.

## 3. Clippy and the guard

Loop until clean:

- `cargo clippy --workspace --target riscv32imac-unknown-none-elf -- -D warnings`. The first run builds `openthread-sys` and takes minutes.
  - An error inside a git dependency means the pinned pair doesn't fit. Find the commit that changed the API with `git -C <clone> log --oneline -S<symbol> OLD..HEAD`, then move one rev. Example: openthread's CSL commit changed `new_with_udp_srp`, so the patch was pinned to the commit before it.
  - An error in `platform/` or `firmware/` is ours to port. The upstream example from step 4 usually shows the new call.
- `bash scripts/lockstep-guard.sh`. For each duplicate, run `cargo tree -i` on every version. Then either move a rev, or, when the source shows the types never meet, change `SEAM_CRATES` and write the reason in the script's comment.
- `cargo fmt --all --check`.

Leave the `--release` build to CI: fat LTO takes about 15 minutes on Windows, and clippy has already caught every type error.

Done when: clippy, the guard and fmt all pass with `--locked`.

## 4. Silent changes

Green clippy doesn't prove the Device works. In the first run, rs-matter-stack stopped serving the Endpoint 0 system clusters. The Platform still compiled, but Commissioning would have failed.

- For each git rev that moved, diff the upstream example closest to our Firmware, for example `git -C <clone> diff OLD NEW -- examples/esp/src/bin/light_thread.rs examples/esp/Cargo.toml`.
- For each crates.io member that bumped, read its `CHANGELOG.md` or `MIGRATING*.md` in the registry source.

Port every behavioural change into `platform/` or `firmware/`, and note each one for the PR.

Then run `bash scripts/adr-version-guard.sh`. For each claim it lists, check the claim in the new version's source. If it still holds, update the marker; if not, amend the ADR (and its line in `docs/architecture.md`). In the first run, ADR-0005's "rs-matter has no ICD Management cluster" went stale unnoticed.

Done when: every hunk of the example diff is either mirrored here or noted as not applying, and the ADR version guard passes.

## 5. PR

Commit, push, and open the PR with the `pr` skill. The body carries:

- the version diff, as a `diff` block with old → new for each member;
- what differed from what was expected going in;
- the API and silent-change fixes;
- the local clippy, guard and fmt results;
- a checklist of on-device checks that is the user's, left unchecked, and never claimed as done:
  - [ ] Flash the Test Board; it boots to the commissioning QR code with no panic
  - [ ] Commission over BLE (Home Assistant needs its Matter Server's test-net DCL option, see README)
  - [ ] Toggle the light over Thread
  - [ ] one item for each silent change from step 4

Bind the PR (`ccd_pr` `get_status`) and offer Auto-fix.

Done when: the PR's CI is green, including the release build and the duplicate guard.
