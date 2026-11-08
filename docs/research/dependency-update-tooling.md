# Dependency update tooling: Renovate vs Dependabot

Research date: 2026-11-08. Scope: the Cargo workspace in this repo (root `Cargo.toml`, `platform/Cargo.toml`, `firmware/on-off-light/Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, `Cargo.lock`).

Source versions used (both tools change often; re-check before relying on line numbers):

| Source | Version / snapshot |
|---|---|
| Renovate docs (docs.renovatebot.com) | page footer "Mend Renovate 44.222.2"; latest release 44.222.2 (2026-11-07) |
| Renovate source | `renovatebot/renovate@812e779` (main, 2026-11-07) |
| GitHub Docs (Dependabot) | `github/docs@8cd1075` (main, 2026-11-06) |
| Dependabot source | `dependabot/dependabot-core@05146d8` (main, 2026-11-07); latest release v0.403.0 (2026-11-03) |
| Cargo Book | doc.rust-lang.org (stable); `rust-lang/cargo@c82e769` (master, 2026-11-07) |
| rustup | 1.29.3 |
| containerbase | `containerbase/base@823b9cc` (main, 2026-11-07) |

Abbreviations in links: **R** = `https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90`, **D** = `https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051`, **GD** = `https://github.com/github/docs/blob/8cd1075179df75319912891d6620c9cb78eefb4d`. All links in this file are written out in full.

## Verdict

**Renovate fits this manifest better.** It is the only one of the two that tracks the `rev`-pinned git dependency in `[workspace.dependencies]`: it proposes the remote's HEAD commit and rewrites the `rev`. It has scheduled whole-lockfile refreshes (`lockFileMaintenance`, which runs `cargo update`). It defaults to lockfile-only updates for in-range versions, which suits the bare and tilde requirements here. It reports a failed `Cargo.lock` update as a PR comment and a red status check. It also installs Rust without rustup, so the Windows-only `rust-toolchain.toml` pin doesn't affect it.

Dependabot handles the crates.io dependencies and `[workspace.dependencies]` well, but it has four problems here:
- It does nothing with a git dependency pinned to a commit SHA, and its workspace-manifest updater can't rewrite git pins.
- It skips the `[patch.crates-io]` crates.
- It has no whole-lockfile refresh.
- Its Cargo updater runs through rustup with a copy of `rust-toolchain.toml`. With the current `stable-x86_64-pc-windows-msvc` pin, that is expected to fail on its Linux runner.

Neither tool would catch the `bt-hci` incompatibility. Cargo resolves the two semver-incompatible versions side by side, so the bot opens a PR with a valid lockfile and only a CI build fails. CI comes before either bot.

Caveat for Renovate: today, the commit it would propose for `rs-matter-embassy` (HEAD of the default branch) is exactly the one the Cargo.toml comments say breaks the build. Put that update behind Dependency Dashboard approval (see point 1).

## Comparison table

| # | Question | Renovate (44.222.x) | Dependabot (core v0.403.x) |
|---|---|---|---|
| 1 | Git dep pinned by `rev` | **Yes.** Extracted as a `git-refs` dependency whose `currentDigest` is the rev. Proposes the remote **HEAD (default branch)** commit as a `digest` update and rewrites the `rev` string. You can turn it off or gate it with `packageRules` (`matchDatasources: ["git-refs"]` / `matchUpdateTypes: ["digest"]`). | **No.** A SHA rev counts as "pinned to a ref that doesn't look like a version", and the update checker returns the current version ("nothing we can do"). The workspace-manifest updater also can't rewrite git `tag`/`rev` (open issue #13219, open PR #14575). |
| 2 | `[patch.crates-io]` git+rev | **Ignored.** The extractor never reads `patch`, so it proposes no updates for them. Cargo commands run in the real checkout, so patches apply and don't break lockfile updates (no known issue). | **Ignored on purpose.** The parser drops patched crate names (`# TODO: Handle patched dependencies`). Git patches go into the temp workspace and Cargo honours them. Only *path* patches pointing outside the repo are a known failure (open issue #14945). |
| 3 | `[workspace.dependencies]` + `dep.workspace = true` | **Supported.** It reads `[workspace.dependencies]` (depType `workspace.dependencies`) and marks member entries `inherited-dependency` (skipped). Bumps land in the root `Cargo.toml`. | **Supported for registry versions.** Member `workspace = true` entries are skipped. A root manifest with `[workspace.dependencies]` goes to `WorkspaceManifestUpdater`, which rewrites `version` strings in the root. It can't rewrite git pins (see 1). |
| 4 | Group the `esp-*` family | `packageRules` with `matchPackageNames: ["esp-*"]` and `groupName`. | `groups:` with `patterns: ["esp-*"]`. |
| 5 | Requirement styles / pre-releases | Default `rangeStrategy=auto` resolves to `update-lockfile` (or `widen` if `<` is present). In-range releases update `Cargo.lock` only. Out-of-range releases replace the requirement, keeping its style (`0.18` → `0.19`, `~1.1.0` → a new tilde range). Also supports `bump`, `replace`, `widen`, `pin`, `in-range-only`. No pre-releases unless the current version is already one (`ignoreUnstable=true`). | Cargo default in source is `increase-if-necessary`: the requirement stays unchanged if it already allows the new version, otherwise it is rewritten with the same precision (`~1.1.0` → `~1.2.2`, `0.18` → `0.19`). Configurable with `versioning-strategy`: `auto`, `increase`, `increase-if-necessary`, `lockfile-only`, `widen`. No pre-releases unless the current version or requirement already contains one. |
| 6 | Lockfile-only refresh (`cargo update`) | **Yes:** `lockFileMaintenance` runs `cargo update`. Off by default; when enabled, the default schedule is "before 4am on monday". In-range updates of direct deps also come as lockfile-only PRs. | **No whole-lockfile refresh.** Per dependency only: `versioning-strategy: lockfile-only` plus `allow: dependency-type: all` (indirect supported for cargo), on the `schedule.interval`. |
| 7 | Unresolvable graph | Two semver-incompatible `bt-hci` versions **resolve**, so the PR opens and only CI catches it. On a real `cargo update` failure, it opens the PR anyway (release older than 2 h, or branch already exists) with a "⚠ Artifact update problem" comment that includes cargo's stderr, plus a failing `renovate/artifacts` status. | `bt-hci` case: same. Cargo resolves, the PR opens, only CI catches it. On a real resolution failure: **no PR**. The job raises `DependencyFileNotResolvable`, and the error only shows in the Dependabot job logs. |
| 8 | Hosting, cost, Windows toolchain pin | Mend Renovate Community Cloud (GitHub App, free, private and public repos; 1 concurrent job, runs every 4 h, 30-min timeout), or self-hosted. Rust comes from containerbase tarballs, not rustup, so the toolchain file is never read; a `stable…` channel sets no Rust constraint. **Not affected.** | Built into GitHub, free. The Cargo updater image ships rustup and writes the repo's toolchain file into its working directory before running `cargo`. A `stable-x86_64-pc-windows-msvc` channel can't run on the Linux updater. **Expected to break; exact error not reproduced.** |

## 1. Git dependencies pinned by `rev`

Concrete case: `rs-matter-embassy = { git = "https://github.com/sysgrok/rs-matter-embassy", rev = "f31233a6…", features = [...] }` in `[workspace.dependencies]`.

### Renovate

- **Detection.** The cargo manager lists `git-refs`, `git-tags`, `github-tags` and `gitlab-tags` as datasources (https://docs.renovatebot.com/modules/manager/cargo/). In the manifest schema, a dependency with `git` goes through `applyGitSource` (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/schema.ts#L60-L68). For `rev`, that sets `datasource = git-refs`, `packageName = <git url>`, `currentDigest = rev`, `replaceString = rev`, and clears any skip reason (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/util.ts#L52-L57). A unit test pins this behaviour: `rev = "abcdef0"` is extracted as `{ datasource: 'git-refs', packageName: 'https://github.com/foo/bar', currentDigest: 'abcdef0' }` (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/extract.spec.ts#L857-L889). Git dependency support landed in PR #36444, merged 2025-09-12 (https://github.com/renovatebot/renovate/pull/36444).
- **Which commit it proposes.** The lookup adds a `digest` update for any dependency with a `currentDigest` (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/workers/repository/process/lookup/index.ts#L592-L602). The digest is resolved with `getDigest(config, newValue)` (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/workers/repository/process/lookup/digest.ts#L171-L174). Cargo `rev` deps have no `currentValue`, so `newValue` is empty, and the git datasource then returns the hash of the remote's `HEAD` from `git ls-remote` (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/datasource/git-refs/base.ts#L160-L174). **Renovate therefore tracks the remote's default branch (`HEAD`).**
- **Rewriting.** `replaceString = rev` means the old SHA string in `Cargo.toml` is replaced with the new one (util.ts link above). Then `Cargo.lock` is regenerated. Because a non-crate dependency changed, Renovate runs `cargo update --config net.git-fetch-with-cli=true --manifest-path Cargo.toml --workspace` (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/artifacts.ts#L33-L40, https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/artifacts.ts#L140-L158).
- **Tracking another branch.** In Cargo, `branch`, `tag` and `rev` are alternatives (https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#choice-of-commit). Renovate skips `branch =` deps with `skipReason: 'git-dependency'` and leaves them to lock file maintenance (util.ts#L58-L62 above). Neither the docs nor the source show an option to make a `rev` pin follow a non-default branch. **Unverified** whether any config can do it.
- **Opt in/out.** It is on by default. To disable or gate it, use `packageRules`:
  - `matchDatasources: ["git-refs"]` or `matchDepNames: ["rs-matter-embassy"]`, combined with `matchUpdateTypes: ["digest"]` (https://docs.renovatebot.com/configuration-options/#packagerulesmatchupdatetypes, https://docs.renovatebot.com/configuration-options/#packagerulesmatchdepnames);
  - then add `"enabled": false` or `"dependencyDashboardApproval": true` (https://docs.renovatebot.com/configuration-options/#dependencydashboardapproval).
  - Note that `packageName` for a git dep is the URL, so match on `depName` (schema.ts above, `if (pkg && !git)`).
- **Recommendation for this repo.** Use `dependencyDashboardApproval` for the `rs-matter-embassy` digest. HEAD is currently the known-incompatible state described in the Cargo.toml comment.

### Dependabot

- **Parsing.** Git declarations become `source: { type: "git", url, branch, ref: tag || rev }` with `requirement: nil` (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/file_parser.rb#L410-L417). The locked "version" of a git package is its commit SHA (same file, #L420-L426).
- **No new commits for a SHA pin.** `latest_git_version_sha`:
  - for an unpinned dependency, it returns the head of the current branch;
  - for a tag that looks like a version, it looks up newer tags;
  - otherwise it returns `dependency.version`, i.e. "nothing we can do" (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/update_checker.rb#L214-L250).
  
  `pinned?` is true when a `ref` is set and isn't a branch name (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/common/lib/dependabot/git_commit_checker.rb#L84-L100). The spec "with a non-version tag" expects the latest version to equal the current one (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/spec/dependabot/cargo/update_checker_spec.rb#L188-L205).
- **Workspace manifest can't rewrite git pins anyway.** `WorkspaceManifestUpdater` only substitutes `version` strings and returns early when old and new requirement are equal or nil, which they always are for git deps (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/file_updater/workspace_manifest_updater.rb#L46-L108). This is tracked as open issue #13219 "Support Cargo workspace manifest git tag update" (2025-10-03, https://github.com/dependabot/dependabot-core/issues/13219) and open PR #14575 (https://github.com/dependabot/dependabot-core/pull/14575).
- **Opt in/out.** There is no setting that makes a SHA-pinned rev update. `ignore: - dependency-name: rs-matter-embassy` would silence it (https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference#ignore--).

## 2. `[patch.crates-io]` with `git` + `rev`

Concrete case: `openthread` and `openthread-sys` patched to `https://github.com/esp-rs/openthread` at `rev = "32ed1bc…"`. In `Cargo.lock` they show as `source = "git+https://github.com/esp-rs/openthread?rev=32ed1bc…#32ed1bc…"`.

### Renovate

- **Not read.** The `CargoManifest` schema only models `dependencies`, `dev-dependencies`, `build-dependencies`, `target.*`, `workspace.dependencies`/`workspace.package` and `package` (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/schema.ts#L103-L132). `extractPackageFile` collects only those sections (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/extract.ts#L188-L242). The docs list the same four tables (https://docs.renovatebot.com/rust/#how-it-works). So **patch entries get no update proposals and don't appear on the dashboard.**
- **Lockfile updates.** Renovate runs the real `cargo update` (precise or `--workspace`) against the checked-out repo (artifacts.ts, linked above), so Cargo applies the patch as usual. I found no issue reporting a git `[patch]` breaking Renovate's cargo artifacts. That is **unverified** by a test run here.
- **Workaround if you want the patch tracked.** Use a regex custom manager with the `git-refs` datasource, as documented for the datasource (https://docs.renovatebot.com/modules/datasource/git-refs/). Not tested here.

### Dependabot

- **Deliberately dropped.** `parse` removes every crate named in any `[patch.*]` table: `dependencies.reject! { |d| patched_dependencies.include?(d.name) }`, with the comment `# TODO: Handle patched dependencies` (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/file_parser.rb#L39-L40, #L247-L253).
- **Lockfile.** The fetcher also collects path patches (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/file_fetcher.rb#L231-L232). The lockfile updater writes the manifests, `Cargo.lock`, `.cargo/config.toml` and the toolchain file into a temp dir and runs `cargo update -p <dep>` (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/file_updater/lockfile_updater.rb#L81-L97, #L428-L441), so a git patch is fetched normally. The known failure is a *path* patch to a sibling repo outside the checkout (open issue #14945, https://github.com/dependabot/dependabot-core/issues/14945). That doesn't apply here.

Cargo semantics for reference: https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html.

## 3. `[workspace.dependencies]` and `dep.workspace = true`

### Renovate

- `[workspace.dependencies]` is extracted with depType `workspace.dependencies` (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/schema.ts#L109-L116, https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/extract.ts#L226-L234; docs: https://docs.renovatebot.com/rust/#how-it-works).
- In member crates, `{ workspace = true }` gets `skipReason: 'inherited-dependency'` (schema.ts#L62-L63; test at https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/extract.spec.ts#L662-L673).
- Updates are applied to the package file the dependency was extracted from, which is the root `Cargo.toml`.
- The lock file is found as a sibling or in a parent directory (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/artifacts.ts#L104-L110).
- `platform = { path = "platform" }` is skipped as `path-dependency` (schema.ts#L60-L61).

### Dependabot

- Member entries with `workspace = true` are skipped (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/file_parser.rb#L136-L152). Root `[workspace.dependencies]` entries become dependencies with group `workspace.dependencies` (#L163-L171).
- If the root manifest has `[workspace.dependencies]`, the file updater sends it to `WorkspaceManifestUpdater` (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/file_updater.rb#L54-L68, #L97-L105). That updater rewrites `name = "x"` and inline `name = { version = "x", … }` in the root, which matches this repo's style (workspace_manifest_updater.rb#L73-L108 above).
- Point `directory: "/"` at the workspace root. Dependabot refuses to run on a member directory (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/file_parser.rb#L106-L118).
- Known gaps don't apply here: the open issues #15410/#16037 concern a root that is *also* a package. This repo's root is a virtual manifest.

## 4. Grouping the `esp-*` family

The nine crates are `esp-hal`, `esp-radio`, `esp-rtos`, `esp-bootloader-esp-idf`, `esp-alloc`, `esp-backtrace`, `esp-println`, `esp-storage` and `esp-metadata-generated`. All are crates.io deps, so name = package name.

### Renovate

Group via `packageRules` and `groupName` (https://docs.renovatebot.com/configuration-options/#groupname). `matchPackageNames` accepts globs via minimatch (https://docs.renovatebot.com/configuration-options/#packagerulesmatchpackagenames, https://docs.renovatebot.com/string-pattern-matching/). There is no built-in esp-rs/embassy preset: `lib/data/monorepo.json` has no entry for them (searched at the pinned commit).

```json
{
  "$schema": "https://docs.renovatebot.com/renovate-schema.json",
  "extends": ["config:recommended"],
  "lockFileMaintenance": { "enabled": true },
  "packageRules": [
    {
      "matchManagers": ["cargo"],
      "matchPackageNames": ["esp-*"],
      "groupName": "esp-hal family"
    },
    {
      "matchManagers": ["cargo"],
      "matchPackageNames": ["embassy-*"],
      "groupName": "embassy"
    },
    {
      "matchManagers": ["cargo"],
      "matchDepNames": ["rs-matter-embassy"],
      "matchUpdateTypes": ["digest"],
      "dependencyDashboardApproval": true
    }
  ]
}
```

Lock file maintenance is never grouped with other updates (note under https://docs.renovatebot.com/configuration-options/#groupname).

### Dependabot

`groups.<id>.patterns` supports `*` wildcards (https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference#patterns-and-exclude-patterns-groups).

```yaml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    groups:
      esp-hal-family:
        patterns:
          - "esp-*"
      embassy:
        patterns:
          - "embassy-*"
```

### Caveat for both tools

Grouping doesn't fix the coupling this repo actually has. Per the crates.io index on 2026-11-08:
- `esp-radio` 0.18.0 requires `esp-hal ~1.1.0-rc.0` and `bt-hci ^0.8.0`;
- `esp-hal` 1.2.2 is the latest stable;
- `esp-radio`'s newest release is the pre-release 1.0.0-beta.1, which needs `esp-hal ~1.2.0` and `bt-hci ^0.9.0` (https://index.crates.io/es/p-/esp-radio, https://index.crates.io/es/p-/esp-hal).

A grouped "esp-hal family" PR would therefore move `esp-hal` to `~1.2.x` but leave `esp-radio` at 0.18, because neither bot proposes the pre-release (point 5). That leaves two `esp-hal` versions in the graph. Side finding, not verified by a build: beta.1's `bt-hci ^0.9.0` may be the release the `esp-radio` comment in `Cargo.toml` is waiting for.

## 5. Requirement styles: tilde, bare, pre-release

Cargo semantics: a bare `0.18` means `^0.18` (`>=0.18.0, <0.19.0`), and `~1.1.0` allows only `1.1.x` (https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#default-requirements, https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#tilde-requirements). Requirements exclude pre-releases unless they name one (https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#pre-releases).

### Renovate

- **Default.** `rangeStrategy=auto` becomes `widen` if the value contains `<`, otherwise `update-lockfile`, "so most upgrades update `Cargo.lock` without changing `Cargo.toml`" (https://docs.renovatebot.com/modules/manager/cargo/; source https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/range.ts). `update-lockfile` = "Update the lock file when in-range updates are available, otherwise `replace` for updates out of range" (https://docs.renovatebot.com/configuration-options/#rangestrategy).
- **In-range** (e.g. `log = "0.4"` → 0.4.x, `esp-hal ~1.1.0` → 1.1.y): a lockfile-only PR via `cargo update --package name@locked --precise new` (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/artifacts.ts#L49-L77).
- **Out-of-range.** The cargo versioning converts bare versions to caret, computes the npm-style new range, and then:
  - keeps tilde/caret precision ("Keep new range precision the same as current");
  - strips the caret it added and truncates to the original component count. Its comment says "`5.0` should be updated to `6.0`", so `0.18` → `0.19` (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/versioning/cargo/index.ts#L98-L162).
  
  For `~1.1.0` → 1.2.2, the result is a tilde range on 1.2. Whether it is `~1.2.0` or `~1.2.2` was **not verified** (it depends on npm versioning's `replace`).
- **Other strategies:** `bump`, `replace`, `widen`, `pin` (Cargo `=x.y.z`, see `getPinnedValue`), `in-range-only` (https://docs.renovatebot.com/configuration-options/#rangestrategy).
- **Pre-releases.** `ignoreUnstable` defaults to `true`: no updates to unstable versions unless the current version is already unstable on the same `major.minor.patch` (https://docs.renovatebot.com/configuration-options/#ignoreunstable; default at https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/config/options/index.ts#L2128-L2134). So `esp-radio = "0.18"` never gets a `1.0.0-beta.x` PR unless a rule sets `ignoreUnstable: false`.

### Dependabot

- **Default in source:** `RequirementsUpdateStrategy::BumpVersionsIfNecessary`. The comment explains: "a bare version is a caret-equivalent (compatible) requirement, so only raise the requirement when it doesn't already allow the new version" (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/update_checker.rb#L148-L159). The GitHub Docs table still describes `auto` generically ("For apps, always increase…"; https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference#versioning-strategy--). For Cargo, the source is authoritative.
- **Rewriting.** If the requirement already allows the target, it is left alone and only the lockfile changes (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/update_checker/requirements_updater.rb#L130-L136). Otherwise `update_version_string` keeps the operator and the number of components: `~1.1.0` → `~1.2.2`, `0.18` → `0.19`, `0.4` → `0.5` (#L140-L152). Pre-release requirements are replaced by the full target version (#L144-L145).
- **Options.** `versioning-strategy` is supported for `cargo`: `auto`, `increase`, `increase-if-necessary`, `lockfile-only`, `widen` (https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference#versioning-strategy--). The Cargo requirements updater accepts `LockfileOnly`, `BumpVersions` and `BumpVersionsIfNecessary` (requirements_updater.rb#L27-L34). **Unverified** what `widen` does for Cargo: it is not in that allowed list.
- **Pre-releases.** `wants_prerelease?` is true only if the current version is a pre-release or a requirement string contains letters (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/update_checker/latest_version_finder.rb#L47-L54). Docs: Cargo follows SemVer pre-release conventions (https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference#versioning-tags). No `1.0.0-beta` proposals for `esp-radio = "0.18"`.

## 6. Lockfile-only updates

### Renovate

- The cargo manager supports lock file maintenance and delegates it to Cargo (`supportsLockFileMaintenance = true`, `lockFileMaintenanceIsDelegatedToPackageManager = true`; https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/index.ts#L14-L16).
- Maintenance runs `cargo update --config net.git-fetch-with-cli=true --manifest-path Cargo.toml` (without `--workspace`), keeping the existing lock file rather than deleting it (artifacts.ts#L28-L47, #L135-L137).
- It is disabled by default. When enabled, the default schedule is `before 4am on monday` and the branch topic is `lock-file-maintenance` (https://docs.renovatebot.com/configuration-options/#lockfilemaintenance; defaults at https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/config/options/index.ts#L2919-L2936).
- On Mend Community Cloud, jobs run every 4 hours (https://docs.renovatebot.com/mend-hosted/overview/#resources-and-scheduling), so the Monday window is hit at most once or twice.
- The `rev`-pinned git deps and the patched crates stay where they are during maintenance, because Cargo keeps `rev` pins (Cargo Book, choice of commit, linked in point 1).

### Dependabot

- There is no "refresh the whole lockfile" mode in the options reference (https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference). Updates are per dependency, on `schedule.interval` (`daily`, `weekly`, `monthly`, `quarterly`, `semiannually`, `yearly` or `cron`; https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference#interval).
- **Closest equivalent:** `versioning-strategy: lockfile-only` ("Only create pull requests to update lockfiles", same page) plus `allow: - dependency-type: "all"`. `indirect`/`all` are supported for `cargo` (https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference#dependency-type-allow). Add a catch-all group (`patterns: ["*"]`) to bundle them. Each dependency still goes through `cargo update -p <dep>` (lockfile_updater.rb#L81-L97). How this behaves with this graph (~390 packages) is **unverified**.

## 7. What a PR looks like when the graph can't resolve

**Key point first.** The `bt-hci` situation described in `Cargo.toml` (one crate needs `bt-hci ^0.8`, another `0.9`) is **not** a resolver failure. Cargo resolves semver-incompatible requirements to two separate versions and builds both. The failure only appears when the compiler sees two distinct `bt-hci` types (https://doc.rust-lang.org/cargo/reference/resolver.html#semver-compatibility, https://doc.rust-lang.org/cargo/reference/resolver.html#version-incompatibility-hazards). Resolution itself fails only for truly conflicting requirements, such as two `=` pins, or for two versions sharing a `links` name (https://doc.rust-lang.org/cargo/reference/resolver.html#links). The crates.io index shows no `links` key on the `esp-hal`, `esp-radio` or `bt-hci` versions checked (index URLs in point 4). So for the scenarios in the comments, **both bots open a normal-looking PR with a consistent `Cargo.lock`, and only a CI build (or a duplicate check) reveals the problem.** Today's graph already has 20 duplicated crates for the firmware target (for example `embassy-sync` 0.6/0.7/0.8, `heapless` 0.8/0.9; measured locally with `cargo tree -d -e normal --target riscv32imac-unknown-none-elf`). A blanket "no duplicates" rule wouldn't work; see the CI sketch.

### Renovate on a real `cargo update` failure

- `updateArtifacts` catches the error, retries a few times for the "package ID specification" case, and otherwise returns an artifact error carrying cargo's stderr (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/cargo/artifacts.ts#L169-L213).
- The branch worker then branches on the age of the release:
  - **release less than 2 hours old and no branch yet:** it raises `MANAGER_LOCKFILE_ERROR` and creates **no PR** in that run;
  - **otherwise:** it commits the manifest change anyway (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/workers/repository/update/branch/index.ts#L480-L497).
- The PR gets a comment titled "⚠ Artifact update problem". It starts "Renovate failed to update an artifact related to this branch", explains the retry triggers, and includes the stderr under "File name: Cargo.lock" (same file #L717-L763).
- A failing status check is set, with context `renovate/artifacts` by default (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/workers/repository/update/branch/artifacts.ts; default name at https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/config/options/index.ts#L478; `statusCheckWhen` docs at https://docs.renovatebot.com/configuration-options/#statuscheckwhen).
- Net effect: the PR is visibly broken. `Cargo.toml` is bumped but `Cargo.lock` is stale, so CI with `--locked` fails too.

### Dependabot on a real resolution failure

- `VersionResolver` maps Cargo failures to errors. When the original requirements are resolvable but the update isn't, it raises `Dependabot::DependencyFileNotResolvable` with cargo's message (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/update_checker/version_resolver.rb#L361-L372, #L449-L453). Some messages are treated as "recoverable" and swallowed, e.g. `all possible versions conflict` (#L416-L440). The lockfile updater does the same (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/file_updater/lockfile_updater.rb#L194-L233).
- **Result: no PR.** For version updates, the error is only visible in the Dependabot job logs. Security-update errors are posted on the alert (https://docs.github.com/en/code-security/reference/supply-chain-security/troubleshoot-dependabot/dependabot-errors#how-to-view-errors).
- In a group, a failing dependency is left out and shown in the logs and job summary (same page, "Failed to update dependency in grouped pull request").

## 8. Operational fit

### Hosting and cost

This repository is **private** with no license (`gh repo view`, 2026-11-08), which matters for the plan choice.

- **Renovate.**
  - Mend Renovate Community Cloud is free "across an unlimited number of public and private repositories", installed as a GitHub App via developer.mend.io. Limits: 1 concurrent job per org, runs every 4 hours, 1 vCPU / 3 GB RAM / 15 GB disk, 30-min job timeout (https://docs.renovatebot.com/mend-hosted/overview/).
  - The "Community (OSS)" tier needs an OSI-approved license, so it doesn't apply to this repo as it stands (same page).
  - The hosted app's Renovate version can lag the open-source release "a few hours to a week" (https://docs.renovatebot.com/mend-hosted/hosted-apps-config/#renovate-version).
  - Self-hosting (CLI, Docker, GitHub Action) is also possible (https://docs.renovatebot.com/bot-comparison/#self-hosting-renovate).
- **Dependabot.**
  - Built into GitHub (https://docs.renovatebot.com/bot-comparison/#built-in-to-github-vs-app) and configured with `.github/dependabot.yml`.
  - Cargo is supported for version updates, security updates, private repositories and private registries, but not vendoring (https://github.com/github/docs/blob/8cd1075179df75319912891d6620c9cb78eefb4d/data/reusables/dependabot/supported-package-managers.md, row "Cargo"; rendered at https://docs.github.com/en/code-security/dependabot/ecosystems-supported-by-dependabot/supported-ecosystems-and-repositories).
  - Standard GitHub-hosted runners are free for Dependabot (https://docs.github.com/en/billing/concepts/product-billing/github-actions).
- **CI minutes for checking bot PRs.** This repo is private, so the CI workflow below draws from the account's included Actions compute usage, whichever bot is used (same billing page).

### Effect of `rust-toolchain.toml` = `stable-x86_64-pc-windows-msvc`

- **Renovate: not affected.**
  - Renovate (via containerbase) installs Rust from the `rust-<version>-x86_64-unknown-linux-gnu` tarball and runs its `install.sh`, linking `cargo` and `rustc` directly. There is no rustup proxy, so the repo's toolchain file is never read by the tool binary (https://github.com/containerbase/base/blob/823b9ccd8f1ce82173f554bb0a009b9d893348e5/src/cli/tools/rust.ts#L60-L111).
  - Renovate itself reads `rust-toolchain(.toml)` only to derive a `rust` version constraint. A channel like `stable` produces no constraint (https://docs.renovatebot.com/modules/manager/cargo/#rust-version, https://docs.renovatebot.com/modules/manager/rust-toolchain/). Its parser accepts the `-<host>` suffix and returns `null` for a named channel (https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/manager/rust-toolchain/extract.ts, `getConstraintValue`; https://github.com/renovatebot/renovate/blob/812e7794d2f86c7892a480c68eecc8ae0084de90/lib/modules/versioning/rust-release-channel/parse.ts).
  - Which Rust version Renovate then uses is **unverified**; presumably containerbase's newest.
  - The `rust-toolchain` manager won't propose changes to a `stable…` channel unless `rangeStrategy: pin` (manager readme, linked above).
- **Dependabot: expected to break.**
  - The Cargo updater image copies a `rust:1.99.0-bookworm` rustup installation (`RUSTUP_HOME=/opt/rust`) (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/Dockerfile#L2-L16).
  - It fetches `rust-toolchain.toml` (renamed to `rust-toolchain`) (https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/file_fetcher.rb#L30-L51, #L503-L508) and writes it next to the manifests before every `cargo` call (lockfile_updater.rb#L428-L434; https://github.com/dependabot/dependabot-core/blob/05146d8f2a50ea4adf6e00a197c85afae99f7051/cargo/lib/dependabot/cargo/update_checker/version_resolver.rb#L329-L334). rustup therefore selects the toolchain from that file (precedence list at https://rust-lang.github.io/rustup/overrides.html).
  - That toolchain targets an MSVC Windows host. rustup's install path checks `host_arch.can_run(target_tuple)` and fails with "toolchain '…' may not be able to run on this system" unless forced (https://github.com/rust-lang/rustup/blob/1.29.3/src/cli/common.rs#L513-L527, https://github.com/rust-lang/rustup/blob/1.29.3/src/errors.rs#L108-L117). The proxy auto-install path passes `force_non_host = true` (https://github.com/rust-lang/rustup/blob/1.29.3/src/config.rs#L568), so the exact failure depends on rustup's code path. It may be a refusal, or a downloaded Windows toolchain whose binaries can't run on Linux.
  - Either way, no working `cargo` is expected. Dependabot's error mapping only recognises *old* toolchains (version_resolver.rb#L456-L467), so this would surface as a generic job error. **Not reproduced; unverified.**
  - **Fix:** change the channel to plain `stable` (see the CI section). The same change is needed for a Linux CI runner anyway.

## CI workflow sketch (not created in the repo)

### Required change to `rust-toolchain.toml`

The current `channel = "stable-x86_64-pc-windows-msvc"` names a Windows host. The documented grammar is `<channel>[-<date>][-<host>]`, and if the host is omitted rustup infers it (https://rust-lang.github.io/rustup/concepts/toolchains.html). Use:

```toml
[toolchain]
channel    = "stable"
components = ["rust-src", "clippy", "rustfmt"]
targets    = ["riscv32imac-unknown-none-elf"]
```

On a Windows machine whose rustup default host is MSVC, `stable` resolves to the same `stable-x86_64-pc-windows-msvc` toolchain. On Linux runners it resolves to the Linux host. The alternative is to leave the file and set `RUSTUP_TOOLCHAIN=stable` in CI, which outranks the toolchain file (https://rust-lang.github.io/rustup/overrides.html). But then `components`/`targets` from the file no longer apply and must be installed explicitly, and Dependabot would still break.

Two notes from this repo's `.cargo/config.toml`:
- On stable, `[unstable] build-std = [...]` is ignored: Cargo only reads the `unstable` table when nightly features are allowed (https://github.com/rust-lang/cargo/blob/c82e769730a4c8559076964b4ebeed021f4aa96e/src/context/mod.rs#L1207-L1220; https://doc.rust-lang.org/cargo/reference/unstable.html). So CI builds against the prebuilt `core`/`alloc` for the target, which `targets` installs.
- `[env] CMAKE_GENERATOR = "Ninja"` and the `openthread-sys` build dependencies (`cmake`, `bindgen`, `cc` in `Cargo.lock`) suggest that a fresh build may compile OpenThread with CMake/Ninja and clang. `openthread-sys` uses prebuilt libraries only when the selected feature knobs match its prebuilt profile; otherwise it builds on the fly (https://github.com/esp-rs/openthread/blob/32ed1bc403ee49b429a377c9a65508af0e33e9f2/openthread-sys/build.rs#L44-L92). Whether this repo's feature set (`openthread-heap-int-8192`, …) hits the prebuilt path is **unverified**, so the sketch installs the native tools.

### `.github/workflows/ci.yml` (sketch)

```yaml
name: CI

on:
  pull_request:
  push:
    branches: [master]

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always
  TARGET: riscv32imac-unknown-none-elf

jobs:
  firmware:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4   # pin to a SHA; the bot will keep it updated

      # Native tools for a possible on-the-fly OpenThread build (cmake + ninja + clang/libclang for bindgen).
      - name: Install native build dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y --no-install-recommends cmake ninja-build clang libclang-dev

      # rustup is preinstalled on GitHub-hosted Ubuntu runners. Install explicitly
      # what rust-toolchain.toml asks for (channel = "stable"), so the step is independent
      # of rustup's auto-install settings.
      - name: Install Rust toolchain
        run: |
          rustup toolchain install stable --profile minimal \
            --component clippy,rust-src --target "$TARGET"
          rustup show

      # --locked: fail if the bot's Cargo.lock is out of sync with Cargo.toml
      # (e.g. a Renovate PR whose artifact update failed).
      - name: Clippy
        run: cargo clippy --workspace --locked --target "$TARGET" -- -D warnings

      # Clippy doesn't link; build catches linker-level breakage (linkall.x, duplicate symbols).
      - name: Build
        run: cargo build --workspace --locked --release --target "$TARGET"

      # The graph already has ~20 legitimately duplicated crates (embassy-sync, heapless, …),
      # so check a deny-list of crates that must exist exactly once.
      - name: Duplicate-dependency guard
        run: |
          dups=$(cargo tree --workspace --locked --target "$TARGET" -e normal -d --depth 0 --prefix none \
                   | awk 'NF {print $1}' | sort | uniq -d \
                   | grep -E '^(bt-hci|trouble-host|esp-[a-z0-9-]+|rs-matter[a-z0-9-]*|openthread(-sys)?)$' || true)
          if [ -n "$dups" ]; then
            echo "::error::Multiple versions in the graph: $(echo $dups | tr '\n' ' ')"
            for d in $dups; do cargo tree --workspace --locked --target "$TARGET" -e normal -i "$d"; done
            exit 1
          fi
```

Notes on the flags:
- `cargo tree -d` lists "only dependencies which come in multiple versions" (https://doc.rust-lang.org/cargo/commands/cargo-tree.html#option-cargo-tree--d). It exits 0 even when it finds some, hence the shell check.
- `--target` defaults to the host platform for `cargo tree`, so the target must be passed (https://doc.rust-lang.org/cargo/commands/cargo-tree.html#option-cargo-tree---target).
- `--locked` errors if Cargo would change the lock file (https://doc.rust-lang.org/cargo/commands/cargo-tree.html#option-cargo-tree---locked).
- Run on `pull_request` so bot PRs get the check.
- To gate merging, add the job as a required status check in branch protection. Renovate's docs recommend this before relying on automerge (https://docs.renovatebot.com/configuration-options/#platformautomerge).

## Open questions and unverified items

1. **Dependabot and the Windows toolchain pin:** the exact failure (rustup refusing a non-host toolchain vs. installing a Windows toolchain that can't run) was not reproduced. The conclusion that no working `cargo` results is inferred from source.
2. **Renovate's Rust version without a constraint:** which version containerbase installs when the toolchain file sets no `rust` constraint is not documented on the pages read.
3. **Renovate with a git `[patch]`:** no evidence of breakage was found, but no test run was done.
4. **Renovate tilde precision:** `~1.1.0` → `~1.2.0` or `~1.2.2` was not verified.
5. **Renovate tracking a non-default branch for a `rev` pin:** no option was found in the docs or source.
6. **Dependabot `versioning-strategy: widen` for Cargo:** the docs list it, but the Cargo `RequirementsUpdater` doesn't allow `WidenRanges`. Behaviour unverified.
7. **Dependabot catch-all group with `dependency-type: all` as a lockfile-maintenance substitute:** behaviour and runtime on this graph are unverified.
8. **Does a two-`esp-hal` graph compile?** `esp-hal` has no `links` key in the index, so Cargo would resolve `esp-hal` 1.1.x and 1.2.x side by side. Whether the build then fails at compile or link time was not tested.
9. **`openthread-sys` prebuilt vs. on-the-fly build on Linux CI:** this decides whether `cmake`/`ninja`/`clang` are really needed. Unverified.
10. **Mend hosted app version:** the docs say it can lag the open-source release by up to a week. Recent cargo/rust-toolchain changes (e.g. PR #47562, merged 2026-11-05) may not be live there yet. They don't change the conclusions above.
