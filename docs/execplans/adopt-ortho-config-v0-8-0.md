# Adopt ortho_config v0.8.0

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE (2026-03-07 UTC)

## Purpose / big picture

Upgrade podbot's `ortho_config` usage from `0.7.0` to `0.8.0` without changing
the intended behaviour of configuration loading. After this work, podbot should
still load configuration with the same precedence order already documented in
`src/config/loader.rs`: defaults, configuration file, environment variables,
then CLI overrides.

Success is observable in three ways. `Cargo.toml` and `Cargo.lock` resolve
`ortho_config` to `0.8.0`. The existing configuration code in
`src/config/types.rs`, `src/config/loader.rs`, and the related tests compile
and pass unchanged in behaviour. The user-facing documentation stops teaching
`0.7.0` snippets and any stale pre-`0.8.0` guidance that conflicts with the
migration notes.

This repository does not currently use `ortho_config` in the default
"derive-generated parser owns the whole CLI" style. Podbot uses a deliberate
manual loader built around `MergeComposer`, with only `AppConfig` deriving
`OrthoConfig`. That matters because several `0.8.0` migration notes are
verification tasks here rather than mandatory code changes. The implementation
must preserve that architecture unless the upgrade proves it no longer works.

## Repository orientation

The implementation should begin by reading these files in this order:

1. `Cargo.toml`, which currently pins `ortho_config = "0.7.0"` and already
   declares `rust-version = "1.88"`.
2. `src/config/types.rs`, where `AppConfig` derives `OrthoConfig` and uses
   `#[ortho_config(prefix = "PODBOT", post_merge_hook)]`.
3. `src/config/loader.rs`, which manually composes defaults, TOML, environment
   variables, and CLI layers using `MergeComposer`.
4. `src/config/cli.rs`, which owns clap parsing and therefore constrains how
   far the upgrade may lean on `ortho_config`'s generated CLI helpers.
5. `src/config/tests/helpers.rs`, `src/config/tests/layer_precedence_tests.rs`,
   `tests/bdd_config_helpers.rs`, and `tests/load_config_integration.rs`, which
   exercise merge behaviour and are the best regression net for this upgrade.
6. `docs/ortho-config-users-guide.md`, which already describes several modern
   `ortho_config` features but still pins installation snippets to `0.7.0` and
   still contains examples that reference `figment::...` directly.

The current audit found no direct `ortho_config_macros` dependency in
`Cargo.toml`, no alias such as `my_cfg = { package = "ortho_config", ... }`, no
`cli_default_as_absent` usage in application code, and no
`[package.metadata.ortho_config]` block for generated documentation artefacts.
Those findings shape the scope below.

## Constraints

- Preserve the current configuration precedence and field names.
- Preserve the existing manual loader architecture in `src/config/loader.rs`
  unless `ortho_config 0.8.0` makes that impossible.
- Do not change user-facing CLI flags, environment variable names, or config
  file keys unless a migration note makes the change mandatory.
- Do not add new external crates. This task is an in-place version upgrade.
- Keep the toolchain requirement at Rust `1.88` or newer. The repository
  already satisfies this with `rust-version = "1.88"`.
- Use `ortho_config` re-exports in application or test code when referring to
  dependencies such as `figment`, `uncased`, `xdg`, or parser modules, unless
  the repository deliberately adds direct dependencies on those crates.
- Do not introduce `cargo orthohelp` metadata unless the repository is already
  generating `OrthoConfigDocs` artefacts or the user explicitly broadens the
  scope to start doing so.
- Keep documentation and comments in en-GB-oxendict spelling.
- Use Makefile targets for final quality gates, and capture long command output
  with `tee` plus `set -o pipefail`.

## Tolerances (exception triggers)

- Scope: if the upgrade requires edits outside `Cargo.toml`, `Cargo.lock`,
  `src/config/`, the directly related tests, and
  `docs/ortho-config-users-guide.md`, stop and ask for confirmation.
- Behaviour: if preserving the current precedence model requires changing the
  public CLI surface or the `PODBOT_*` environment variable contract, stop and
  ask for confirmation.
- Architecture: if `ortho_config 0.8.0` no longer supports the current manual
  `MergeComposer`-based approach and a rewrite to generated `load()` becomes
  necessary, stop and ask for confirmation.
- Dependencies: if the upgrade requires adding a direct
  `ortho_config_macros`, `figment`, `uncased`, `xdg`, YAML, or JSON5
  dependency, stop and ask for confirmation.
- Documentation artefacts: if `cargo orthohelp` becomes necessary to keep the
  build green, stop and confirm whether generated docs are now in scope.
- Iterations: if compile or test failures remain after two focused repair
  rounds, stop and document the remaining errors before proceeding.

## Risks

- Risk: `ortho_config 0.8.0` may change derive-generated trait bounds, method
  names, or helper types used by `AppConfig::merge_from_layers`. Severity:
  medium. Likelihood: medium. Mitigation: bump the dependency first, run a
  narrow compile check, and adapt only the affected config code before touching
  wider documentation.

- Risk: documentation currently teaches stale version pins and a few raw
  `figment::` examples, so the code may upgrade cleanly while the docs remain
  subtly wrong. Severity: medium. Likelihood: high. Mitigation: treat
  `docs/ortho-config-users-guide.md` as part of the upgrade, not as optional
  polish.

- Risk: the `0.8.0` migration notes mention YAML 1.2 parsing changes, but this
  repository currently loads TOML in `src/config/loader.rs`. Severity: low.
  Likelihood: low. Mitigation: verify that podbot does not enable the `yaml`
  feature today; if it does not, record the note as not currently applicable
  rather than adding speculative YAML coverage.

- Risk: the `cli_default_as_absent` migration note could prompt unnecessary
  code churn even though podbot does not use that attribute in application
  code. Severity: low. Likelihood: medium. Mitigation: grep for the attribute
  and for clap defaults, then only change code or docs where the migration note
  actually applies.

- Risk: `cargo update` may refresh unrelated transitive dependencies in
  `Cargo.lock`. Severity: low. Likelihood: medium. Mitigation: use
  `cargo update -p ortho_config --precise 0.8.0`, inspect the resulting diff,
  and do not hand-edit `Cargo.lock`.

## Plan of work

### Milestone 1: establish the baseline and trigger the upgrade

Update `Cargo.toml` to `ortho_config = "0.8.0"` and refresh the lockfile with
the narrowest possible Cargo command so the diff stays reviewable. Confirm that
the lockfile resolves both `ortho_config` and the transitive
`ortho_config_macros` package to `0.8.0`.

Immediately after the version bump, run a narrow compile-oriented command to
surface the first real incompatibilities before editing code broadly. The goal
of this milestone is to produce a concrete list of breakages, not to guess at
them from the migration notes.

### Milestone 2: repair compile-time compatibility in config code

Resolve any breakages in `src/config/types.rs`, `src/config/loader.rs`, and the
directly related test helpers. Keep the existing `Cli` type in
`src/config/cli.rs` as the source of truth for command parsing unless the
compiler proves otherwise.

Verify each migration note explicitly:

1. Dependency versions: `Cargo.toml` and `Cargo.lock` must be on `0.8.0`.
2. Rust version: already satisfied; confirm it does not need to move past
   `1.88`.
3. Crate alias attribute: no alias exists today, so no
   `#[ortho_config(crate = "...")]` attribute should be added unless the
   implementation introduces an alias deliberately.
4. `cli_default_as_absent`: podbot application code does not currently use the
   attribute, so no runtime behaviour change is expected. Only fix examples or
   tests if a real usage is discovered.
5. Re-export guidance: application code should continue to use
   `ortho_config::serde_json` and `ortho_config::toml`, and any new direct use
   of `figment`, `uncased`, or `xdg` should prefer `ortho_config::...` paths.

Do not rewrite the manual loader into `AppConfig::load()` or `compose_layers()`
merely because those helpers exist. That would be a behavioural refactor, not a
dependency upgrade.

### Milestone 3: reconcile repository documentation with 0.8.0

Update `docs/ortho-config-users-guide.md` so it no longer instructs readers to
depend on `0.7.0`. Review the migration-note-sensitive sections and align them
with the actual `0.8.0` rules:

1. Installation snippets should show `0.8.0`.
2. Any examples that discuss `cli_default_as_absent` should keep using typed
   clap defaults such as `default_value_t`.
3. Examples that mention `figment`, `uncased`, or `xdg` only as
   `ortho_config`-related implementation detail should prefer
   `ortho_config::figment`, `ortho_config::uncased`, and `ortho_config::xdg`
   paths.
4. The YAML 1.2 notes should remain accurate and should not imply that podbot
   itself currently loads YAML unless the code has changed to do so.

If no repository workflow currently generates `OrthoConfigDocs` metadata, note
that the `cargo orthohelp` migration step is not applicable in this project at
present. Do not invent a new doc-generation pipeline as part of this upgrade.

### Milestone 4: validate behaviour and quality gates

Once the code and docs are updated, run formatting and the full project gates.
Use `tee` for every long-running command so the logs can be inspected after the
fact without losing the exit code.

Use these commands exactly, from the repository root:

```sh
set -o pipefail && cargo update -p ortho_config --precise 0.8.0 \
  2>&1 | tee /tmp/adopt-ortho-config-v0-8-0-cargo-update.log
```

```sh
set -o pipefail && cargo check --all-targets --all-features \
  2>&1 | tee /tmp/adopt-ortho-config-v0-8-0-cargo-check.log
```

```sh
set -o pipefail && make fmt \
  2>&1 | tee /tmp/adopt-ortho-config-v0-8-0-fmt.log
```

```sh
set -o pipefail && make check-fmt \
  2>&1 | tee /tmp/adopt-ortho-config-v0-8-0-check-fmt.log
```

```sh
set -o pipefail && make lint \
  2>&1 | tee /tmp/adopt-ortho-config-v0-8-0-lint.log
```

```sh
set -o pipefail && make test \
  2>&1 | tee /tmp/adopt-ortho-config-v0-8-0-test.log
```

```sh
set -o pipefail && MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint \
  2>&1 | tee /tmp/adopt-ortho-config-v0-8-0-markdownlint.log
```

```sh
set -o pipefail && make nixie \
  2>&1 | tee /tmp/adopt-ortho-config-v0-8-0-nixie.log
```

The implementation is complete only when all of those commands exit `0` and the
config behaviour still matches the existing tests.

## Acceptance checks

The final implementation should leave the repository in a state where a novice
can verify success with these observations:

1. `Cargo.toml` shows `ortho_config = "0.8.0"`.
2. `Cargo.lock` resolves `ortho_config` and `ortho_config_macros` to `0.8.0`.
3. `src/config/loader.rs` still documents and implements the precedence order
   defaults < file < environment < CLI.
4. `docs/ortho-config-users-guide.md` no longer advertises `0.7.0`.
5. `make check-fmt`, `make lint`, `make test`, `make markdownlint`, and
   `make nixie` all pass.

## Progress

- [x] (2026-03-07 UTC) Audited the current repository state before writing this
  plan. Confirmed `rust-version = "1.88"`, `ortho_config = "0.7.0"`, one
  `OrthoConfig` derive in `src/config/types.rs`, and a manual `MergeComposer`
  loader in `src/config/loader.rs`.
- [x] (2026-03-07 UTC) Confirmed there is no direct `ortho_config_macros`
  dependency, no `ortho_config` crate alias, no application use of
  `cli_default_as_absent`, and no `[package.metadata.ortho_config]` block.
- [x] (2026-03-07 UTC) Confirmed `docs/ortho-config-users-guide.md` already
  covers YAML 1.2 and re-export concepts, but still contains `0.7.0` install
  snippets and raw `figment::` examples that should be reviewed during the
  upgrade.
- [x] (2026-03-07 UTC) Updated `Cargo.toml` and `Cargo.lock` so
  `ortho_config` and `ortho_config_macros` both resolve to `0.8.0`.
- [x] (2026-03-07 UTC) Ran `cargo check --all-targets --all-features` against
  `ortho_config 0.8.0`; the repository compiled without any Rust source changes.
- [x] (2026-03-07 UTC) Updated `docs/ortho-config-users-guide.md` to use
  `0.8.0` dependency snippets, `ortho_config::figment` re-export examples, and
  `0.8.0` migration guidance.
- [x] (2026-03-07 UTC) Ran the full applicable quality gates:
  `make fmt`, `make check-fmt`, `make lint`, `make test`,
  `MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint`, and `make nixie`.

## Surprises & Discoveries

- (2026-03-07 UTC) Podbot already satisfies the new minimum toolchain because
  `Cargo.toml` declares `rust-version = "1.88"`.
- (2026-03-07 UTC) The repository does not rely on `ortho_config` to own clap
  parsing. `src/config/cli.rs` owns the CLI, while `src/config/loader.rs`
  manually composes merge layers. This means many migration notes are checks,
  not automatic edits.
- (2026-03-07 UTC) No direct aliasing of the `ortho_config` crate was found, so
  the new `#[ortho_config(crate = "...")]` attribute is not expected to be
  necessary.
- (2026-03-07 UTC) No current workflow references `cargo orthohelp`,
  `OrthoConfigDocs`, or `[package.metadata.ortho_config]`. The documentation
  artefact migration step is therefore currently non-applicable unless later
  evidence contradicts this.
- (2026-03-07 UTC) `cargo check --all-targets --all-features` succeeded
  immediately after the dependency bump. For podbot, this upgrade is a
  dependency and documentation alignment task rather than a code-compatibility
  repair.
- (2026-03-07 UTC) `make typecheck` is not present in the current Makefile
  even though an older project-memory note claimed it had been added. The real
  repository gates remain `check-fmt`, `lint`, `test`, `markdownlint`, and
  `nixie`.

## Decision Log

- Decision: keep this upgrade scoped to dependency compatibility, tests, and
  documentation that directly references `ortho_config`. Rationale: the
  migration notes do not justify a broader rearchitecture, and the existing
  manual loader was a deliberate earlier design choice. Date/author: 2026-03-07
  / Codex.

- Decision: treat `docs/ortho-config-users-guide.md` as first-class upgrade
  scope. Rationale: it still teaches `0.7.0` installation snippets, and
  migration guidance is only useful if the repository documentation matches the
  code. Date/author: 2026-03-07 / Codex.

- Decision: do not add `#[ortho_config(crate = "...")]` unless an actual crate
  alias appears during implementation. Rationale: no alias exists in the
  current repository, and adding the attribute pre-emptively would be noise
  rather than a compatibility fix. Date/author: 2026-03-07 / Codex.

- Decision: keep `cargo orthohelp` metadata out of the initial implementation.
  Rationale: no current repository workflow generates `OrthoConfigDocs`
  artefacts, so adding that metadata would broaden scope beyond the observed
  upgrade need. Date/author: 2026-03-07 / Codex.

- Decision: do not change `src/config/types.rs` or `src/config/loader.rs` after
  the `0.8.0` bump compiled cleanly. Rationale: the current manual
  `MergeComposer` architecture remains compatible, so any further Rust edits
  would be churn rather than migration work. Date/author: 2026-03-07 / Codex.

## Outcomes & Retrospective

The implementation changed three areas:

- `Cargo.toml` now pins `ortho_config = "0.8.0"`.
- `Cargo.lock` now resolves both `ortho_config` and `ortho_config_macros` to
  `0.8.0`.
- `docs/ortho-config-users-guide.md` now uses `0.8.0` dependency snippets,
  explains the `0.7.x` to `0.8.0` migration points relevant to this repository,
  and updates `figment` examples to use `ortho_config` re-exports.

The final validation results were:

- `cargo check --all-targets --all-features`: passed.
- `make fmt`: passed.
- `make check-fmt`: passed.
- `make lint`: passed. A pre-existing rustdoc warning about
  `missing_crate_level_docs` being renamed to
  `rustdoc::missing_crate_level_docs` still appears, but it does not fail the
  gate and was not introduced by this upgrade.
- `make test`: passed.
- `MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint`: passed.
- `make nixie`: passed.

Migration notes that turned out to be non-applicable in podbot:

- No crate alias is used, so `#[ortho_config(crate = "...")]` was not needed.
- Podbot does not use `cli_default_as_absent` in application code, so no clap
  default migration was needed in Rust sources.
- Podbot still loads TOML rather than YAML in `src/config/loader.rs`, so the
  YAML 1.2 behaviour change is documentation-only context here.
- The repository does not generate `OrthoConfigDocs` artefacts, so
  `[package.metadata.ortho_config]` and `cargo orthohelp` remain out of scope.

No user-visible runtime behaviour changed. Configuration precedence remains
defaults < file < environment < CLI, and the upgrade completed without Rust
source changes.
