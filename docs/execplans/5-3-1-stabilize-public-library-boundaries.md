# Stabilize public library boundaries

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose / big picture

After this change, Podbot's public library API surface is explicitly
documented, feature-gated, and integration-tested so that another Rust crate
can depend on `podbot` as a library with:

- documented, versioned public modules and request/response types,
- semantic errors (`PodbotError`) exclusively (no `eyre` types in public
  signatures),
- gated CLI module visibility via the `cli` feature (note: `clap` remains a
  transitive dependency via `ortho_config` regardless of feature settings),
- reconciled hook and validation schemas that match the documented
  integration contract, and
- integration tests that exercise Podbot as a library dependency from a
  host-style call path.

Observable success:

1. `make check-fmt && make lint && make test` all pass.
2. A new `tests/library_embedding.rs` integration test drives
   `podbot::api`, `podbot::config`, `podbot::engine`, and `podbot::error`
   from a host-application call path, proving that the library surface is
   self-contained and usable without CLI types.
3. `podbot::cli` is gated behind a Cargo feature `cli` (enabled by
   default) to control module visibility. Note: `ortho_config` maintains
   an unconditional dependency on `clap`, so the feature gates API surface
   only.
4. All public modules have a documented API reference in
   `docs/podbot-design.md` under a "Public library API reference"
   section.
5. Hook and validation schema types referenced in `docs/podbot-design.md`
   are either already present in the public surface or explicitly marked
   as future-planned with a tracking reference.
6. New unit tests (`rstest`) and behavioural tests (`rstest-bdd` v0.5.0)
   cover the feature-gating boundary, library embedding paths, and error
   type contract.
7. `docs/users-guide.md` documents the `cli` feature flag and library
   embedding instructions.
8. Roadmap Step 5.3 is marked done in `docs/podbot-roadmap.md`.

This is Step 5.3 of Phase 5 in the roadmap (`docs/podbot-roadmap.md`).

## Agent team (planning + implementation)

1. **Coordinator (lead)**
   - Owns milestone sequencing and tolerance enforcement.
   - Ensures all quality gates run before the end of the turn.
2. **API surface auditor**
   - Enumerates all `pub` items in library modules.
   - Verifies no `eyre` types leak into public signatures.
   - Documents each public module's supported types and functions.
3. **Feature-gate implementer**
   - Introduces the `cli` Cargo feature.
   - Gates `pub mod cli` behind `#[cfg(feature = "cli")]`.
   - Ensures the binary enables the feature in its build path.
   - Verifies `cargo check --no-default-features` compiles without
     `clap`.
4. **Schema reconciliation steward**
   - Reviews hook and validation types against `docs/podbot-design.md`.
   - Adds placeholder types or tracking annotations for unimplemented
     schema surfaces.
5. **Testing lead**
   - Adds `rstest` unit tests for feature-gating and error contracts.
   - Adds `rstest-bdd` behavioural tests for library embedding paths.
   - Adds integration tests that exercise the library from a host-style
     call path.
6. **Documentation owner**
   - Updates `docs/podbot-design.md` with the public API reference.
   - Updates `docs/users-guide.md` with library embedding guidance.
   - Marks roadmap Step 5.3 as done.

## Constraints

Hard invariants that must hold throughout implementation. Violation requires
escalation, not workarounds.

- Files must be fewer than 400 lines each.
- Every module must begin with a `//!` module-level doc comment.
- en-GB-oxendict spelling ("-ize" / "-yse" / "-our") in all comments and
  documentation.
- No `unwrap()` or `expect()` in production code (clippy denies
  `unwrap_used`, `expect_used`).
- No `println!` or `eprintln!` in library code (clippy denies
  `print_stdout`, `print_stderr`).
- Library public API signatures must not expose `eyre::Report` or
  `eyre::Result`. Only `podbot::error::Result<T>` and `PodbotError`.
- `pub mod cli` must be conditionally compiled behind the `cli` Cargo
  feature (enabled by default).
- The binary crate must enable the `cli` feature.
- Note: `cargo check --no-default-features` will still pull in `clap` as a
  transitive dependency via `ortho_config`. The `cli` feature gates module
  visibility, not dependency removal.
- No new external crate dependencies may be added.
- Existing public API signatures in `podbot::api`, `podbot::config`, and
  `podbot::engine` must not change in a breaking way.
- `rstest` for unit tests; `rstest-bdd` v0.5.0 for behavioural tests.
- BDD step function parameter names must match fixture names exactly.
- BDD feature files must use unquoted text for `{param}` captures.
- BDD tests must use `StepResult<T> = Result<T, String>` pattern.
- `make check-fmt`, `make lint`, `make test` must pass before any commit.
- Commit messages use imperative mood; atomic commits; one logical unit
  per commit.

## Tolerances (exception triggers)

- **Scope**: if implementation requires changes to more than 25 files or
  1,500 net lines of code, stop and escalate.
- **Interface**: if any existing public API signature in `podbot::api`,
  `podbot::config`, or `podbot::engine` must change in a breaking way,
  stop and escalate.
- **Dependencies**: if a new external crate dependency is required, stop
  and escalate.
- **Iterations**: if tests still fail after 3 focused fix attempts on a
  single issue, stop and escalate.
- **Ambiguity**: if multiple valid interpretations exist for a schema
  reconciliation decision, stop and present options.

## Risks

- Risk: Feature-gating `pub mod cli` may break downstream code that
  imports `podbot::cli` types without enabling the feature.
  Severity: low
  Likelihood: low (the binary enables default features)
  Mitigation: the `cli` feature is enabled by default, so existing
  consumers are unaffected. Library-only consumers must opt in by
  specifying `default-features = false`.

- Risk: Gating `clap` behind a feature may trigger conditional-compilation
  issues in `main.rs` or integration tests that use CLI types.
  Severity: medium
  Likelihood: medium
  Mitigation: `main.rs` compiles as a binary target, which always gets
  default features. Integration tests that import `podbot::cli` will need
  the `cli` feature; since dev-dependencies inherit default features this
  should work automatically. If not, add `features = ["cli"]` to the
  dev-dependency.

- Risk: Hook and validation schemas referenced in the design doc may not
  yet exist as Rust types, requiring placeholder stubs.
  Severity: low
  Likelihood: high (these are Phase 4.8 and 4.9 features, not yet
  implemented)
  Mitigation: document the planned schemas in the design doc with
  explicit "future-planned" annotations and roadmap references. Do not
  create stub types that would mislead consumers.

- Risk: `cfg(feature = "cli")` gating on `pub mod cli` might cause
  `missing_docs` warnings on the conditional module declaration.
  Severity: low
  Likelihood: medium
  Mitigation: ensure the `#[cfg(feature = "cli")]` attribute is placed
  correctly and the module doc comment satisfies the lint.

## Progress

- [x] Stage A: Audit public API surface and document supported modules.
- [x] Stage B: Ensure public APIs use semantic errors exclusively.
- [x] Stage C: Gate CLI-only dependencies behind `cli` feature boundary.
- [x] Stage D: Reconcile hook and validation schemas with design doc.
- [x] Stage E: Add integration tests for library embedding.
- [x] Stage F: Documentation updates and roadmap completion.
- [x] Stage G: Final quality gate verification.

## Decision log

- Decision: Gate the `cli` module behind a Cargo feature rather than
  moving it to a separate crate.
  Rationale: A feature flag is the simplest approach that achieves the
  goal of hiding CLI adapter types from library consumers.
  A separate crate would add workspace management complexity without
  proportional benefit at this stage. The feature can be refined later
  if multi-crate extraction is warranted. Note: `clap` remains a
  transitive dependency through `ortho_config` and cannot be made
  optional at this time; the feature controls module visibility only.
  Date/Author: 2026-04-07 (agent)

- Decision: Use `default = ["cli"]` so the feature is opt-out rather
  than opt-in.
  Rationale: Existing consumers (the binary, integration tests, and any
  downstream users) continue working without changes. Only library-only
  consumers need to set `default-features = false`. This follows the
  Cargo convention for features that most consumers need.
  Date/Author: 2026-04-07 (agent)

- Decision: Document hook and validation schemas as "future-planned"
  rather than creating stub types.
  Rationale: Phase 4.8 (prompt, bundle, and validation surfaces) and
  Phase 4.9 (hook execution) are not yet implemented. Creating stub
  Rust types would add dead code that misleads consumers. The design
  doc already describes these schemas; adding explicit "future-planned"
  annotations with roadmap references is sufficient for the
  stabilization contract.
  Date/Author: 2026-04-07 (agent)

- Decision: Keep `pub mod github` in the public API surface but mark it
  as "internal, subject to change" in documentation.
  Rationale: The GitHub module exposes types like `GitHubAppClient`
  trait and `validate_app_credentials` that library embedders may need
  for GitHub App integration. However, the API is not yet stable.
  Marking it as internal-but-public avoids breaking existing usage
  while signalling that the surface may change.
  Date/Author: 2026-04-07 (agent)

## Surprises & discoveries

- Discovery: `ortho_config` v0.8.0 unconditionally depends on `clap`
  (non-optional). The `OrthoConfig` derive macro generates code
  referencing `clap::Parser`. This means `clap` cannot be made optional
  in podbot's `Cargo.toml` because the derive macro on `AppConfig`
  requires it at compile time.
  Impact: the `cli` feature gates the `pub mod cli` module (visibility
  of Clap parse types) rather than the `clap` dependency itself. `clap`
  remains a transitive dependency through `ortho_config`. Library-only
  consumers still benefit because they don't need to interact with
  `podbot::cli` types.
  Mitigation: added `required-features = ["cli"]` to the `[[bin]]`
  target so `cargo check --no-default-features` compiles the library
  without the binary.
  Date: 2026-04-07

## Outcomes & retrospective

Completed successfully on 2026-04-07. All stages implemented as planned
with one significant discovery: `ortho_config` v0.8.0 unconditionally
depends on `clap`, preventing the `clap` crate from being made optional.
The `cli` feature was implemented to gate module visibility instead.

Key outcomes:

1. Public API reference section added to `docs/podbot-design.md`.
2. Planned API surfaces section documents unimplemented schemas.
3. `cli` Cargo feature gates `pub mod cli` (enabled by default).
4. `[[bin]]` target declares `required-features = ["cli"]`.
5. `cargo check --no-default-features --lib` compiles cleanly.
6. 6 new integration tests in `tests/library_embedding.rs`.
7. 4 new BDD scenarios in `tests/features/library_boundary.feature`.
8. 2 new unit tests for error type contracts in `src/error.rs`.
9. 1 new feature gate test in `src/lib.rs`.
10. `docs/users-guide.md` updated with library embedding section.
11. Roadmap Step 5.3 marked done.
12. All quality gates pass: `make check-fmt`, `make lint`, `make test`.

## Context and orientation

### Current state (as of 2026-04-07)

Podbot is delivered as both a CLI binary and a Rust library. Steps 5.1
and 5.2 are complete:

- **Step 5.1** extracted command orchestration into `podbot::api` with
  `CommandOutcome`, `ExecParams`, `exec()`, and stub functions for
  `run_agent`, `list_containers`, `stop_container`, `run_token_daemon`.
- **Step 5.2** decoupled configuration loading from Clap by introducing
  `ConfigLoadOptions`, `ConfigOverrides`, and
  `load_config`/`load_config_with_env` in `podbot::config`, while
  keeping Clap parse types in `podbot::cli`.

### Current public module structure

```plaintext
podbot::
  api::           CommandOutcome, ExecParams, exec, run_agent,
                  list_containers, stop_container, run_token_daemon
  cli::           Cli, Commands, RunArgs, HostArgs, ExecArgs, StopArgs,
                  TokenDaemonArgs, AgentKindArg, AgentModeArg
  config::        AppConfig, AgentConfig, AgentKind, AgentMode,
                  ConfigLoadOptions, ConfigOverrides, CredsConfig,
                  GitHubConfig, McpConfig, McpAllowedOriginPolicy,
                  McpAuthTokenPolicy, McpBindStrategy, SandboxConfig,
                  SelinuxLabelMode, WorkspaceConfig, WorkspaceSource,
                  CommandIntent, load_config, load_config_with_env,
                  env_var_names
  engine::        EngineConnector, SocketResolver, SocketPath,
                  ContainerExecClient, ContainerCreator,
                  ContainerUploader, ExecMode, ExecRequest, ExecResult,
                  CreateContainerRequest, ContainerSecurityOptions,
                  CredentialUploadRequest, CredentialUploadResult,
                  SelinuxLabelMode, McpAllowedOriginPolicy,
                  McpAuthTokenPolicy, McpBindStrategy, McpConfig,
                  CreateContainerFuture, CreateExecFuture,
                  InspectExecFuture, ResizeExecFuture,
                  StartExecFuture, UploadToContainerFuture
  error::         PodbotError, ConfigError, ContainerError,
                  GitHubError, FilesystemError, Result<T>
  github::        load_private_key, build_app_client,
                  validate_app_credentials, GitHubAppClient,
                  OctocrabAppClient, BoxFuture
```

### Key files

- `Cargo.toml` — no features section currently exists
- `src/lib.rs` — exports `api`, `cli`, `config`, `engine`, `error`,
  `github`
- `src/main.rs` — CLI binary adapter
- `src/cli/mod.rs` — Clap parse types
- `src/error.rs` — semantic error hierarchy
- `docs/podbot-design.md` — architecture and module structure
- `docs/users-guide.md` — operator documentation
- `docs/podbot-roadmap.md` — roadmap (target: Step 5.3)

### Existing patterns to follow

- Feature gating: Cargo features with `#[cfg(feature = "...")]` on
  module declarations. The binary target enables default features.
- Module structure: `src/<module>/mod.rs` re-exports from submodules.
- Dependency injection: traits like `ContainerExecClient` and
  `mockable::Env` for testability.
- BDD tests: `tests/bdd_<name>.rs` with
  `tests/bdd_<name>_helpers/{mod,state,steps,assertions}.rs` and
  `tests/features/<name>.feature`.
- Error handling: library returns `podbot::error::Result<T>`; CLI
  converts to `eyre::Report` at the boundary.

## Plan of work

### Stage A: Audit and document public API surface

**Goal:** Enumerate and document all public modules, types, traits, and
functions in the library surface.

#### A.1: Audit public API surface

Enumerate all `pub` items in library modules (`api`, `config`, `engine`,
`error`, `github`). Verify:

1. No `eyre` types appear in public function signatures or public type
   fields.
2. All public items have `///` doc comments (enforced by
   `missing_docs = "deny"`).
3. All modules have `//!` module-level doc comments.

The `cli` module is currently public but will be feature-gated in Stage
C. It is acceptable for `cli` types to reference `clap` types since the
module is behind the feature gate.

#### A.2: Document public modules in design doc

Add a "Public library API reference" section to `docs/podbot-design.md`
listing each public module with its supported types and functions. Use a
table format:

| Module     | Stability | Types and functions                     |
| ---------- | --------- | --------------------------------------- |
| `api`      | Stable    | `CommandOutcome`, `ExecParams`, `exec`, |
|            |           | `run_agent`, `list_containers`,         |
|            |           | `stop_container`, `run_token_daemon`    |
| `config`   | Stable    | `AppConfig`, `ConfigLoadOptions`, ...   |
| `engine`   | Stable    | `EngineConnector`, `ExecRequest`, ...   |
| `error`    | Stable    | `PodbotError`, `ConfigError`, ...       |
| `github`   | Internal  | Subject to change; not part of the      |
|            |           | stable integration contract             |

**Stage A gate**: no code changes; documentation-only. Run
`make markdownlint` if documentation files are modified.

### Stage B: Ensure semantic errors across public APIs

**Goal:** Confirm that all public API functions return
`podbot::error::Result<T>` with `PodbotError` variants, and that no
`eyre::Report` or `eyre::Result` appears in public library signatures.

#### B.1: Audit error return types

Scan all `pub fn` declarations in library modules for return types. The
audit from context gathering confirms:

- `podbot::api::exec()` returns `PodbotResult<CommandOutcome>` (alias
  for `Result<CommandOutcome, PodbotError>`). Correct.
- `podbot::api::{run_agent, list_containers, stop_container,
  run_token_daemon}` return `PodbotResult<CommandOutcome>`. Correct.
- `podbot::config::{load_config, load_config_with_env}` return
  `crate::error::Result<AppConfig>`. Correct.
- `podbot::engine::EngineConnector::connect()` returns
  `Result<Docker, PodbotError>`. Correct.
- `podbot::github::{load_private_key, build_app_client}` return
  `Result<T, GitHubError>`. These use domain-specific errors, not
  `PodbotError`. This is acceptable: `GitHubError` is a variant of
  `PodbotError` and callers can convert with `?`.

No action required if the audit confirms all return types are semantic.
If any function returns `eyre::Result`, refactor it to use
`podbot::error::Result` or a domain error enum.

#### B.2: Add unit test for error type contract

Add a compile-time test (or static assertion) in `src/error.rs` tests
confirming that `PodbotError` implements `std::error::Error + Send +
Sync + 'static`. This ensures the error type is suitable for use in
async contexts and across thread boundaries.

**Stage B gate**: `make check-fmt && make lint && make test`.

### Stage C: Gate CLI-only dependencies behind feature boundary

**Goal:** Introduce a Cargo feature `cli` (enabled by default) that
gates `pub mod cli` and the `clap` dependency. Library-only consumers
can set `default-features = false` to avoid pulling in `clap`.

#### C.1: Add `[features]` section to `Cargo.toml`

```toml
[features]
default = ["cli"]
cli = ["dep:clap"]
```

Change the `clap` dependency from:

```toml
clap = { version = "4.5.60", features = ["derive"] }
```

to:

```toml
clap = { version = "4.5.60", features = ["derive"], optional = true }
```

#### C.2: Gate `pub mod cli` in `src/lib.rs`

Change:

```rust
pub mod cli;
```

to:

```rust
#[cfg(feature = "cli")]
pub mod cli;
```

Ensure the module-level doc comment for `lib.rs` conditionally lists the
`cli` module:

```rust
//! - [`cli`]: `Clap` parse types for the `podbot` binary (CLI adapter layer)
//!   (requires the `cli` feature, enabled by default)
```

#### C.3: Verify `main.rs` compiles with default features

The binary target inherits default features, so `use podbot::cli::*`
statements in `main.rs` should compile without changes. Verify with:

```bash
cargo build
```

#### C.4: Verify library compiles without CLI feature

```bash
cargo check --no-default-features
```

This should succeed, proving the library surface is usable without
`clap`.

#### C.5: Gate integration tests that use CLI types

Any integration test files in `tests/` that import `podbot::cli::*` need
a `#[cfg(feature = "cli")]` attribute or a feature requirement. Since
dev-dependencies inherit default features, this should work automatically.
Verify with `make test`.

#### C.6: Add unit test for feature gate

Add a test in `src/lib.rs` that verifies the `cli` module is available
when the feature is enabled:

```rust
#[cfg(test)]
#[cfg(feature = "cli")]
mod cli_feature_tests {
    #[test]
    fn cli_module_is_available() {
        // Compile-time proof that podbot::cli is available.
        let _ = std::any::type_name::<crate::cli::Cli>();
    }
}
```

**Stage C gate**: `make check-fmt && make lint && make test` AND
`cargo check --no-default-features`.

### Stage D: Reconcile hook and validation schemas

**Goal:** Ensure that the public hook and validation schemas referenced
in `docs/podbot-design.md` are either present in the library surface or
explicitly marked as future-planned.

#### D.1: Audit design doc for referenced schema types

The design doc references several schema concepts that are not yet
implemented as Rust types:

- `LaunchRequest` / `LaunchPlan` (Step 4.5, not implemented)
- Hook artefact and subscription models (Step 4.9, not implemented)
- Prompt frontmatter and bundle manifest contracts (Step 4.8, not
  implemented)
- `validate_prompt` function (Step 4.8, not implemented)
- MCP wire request/response models (Step 4.7, not implemented)
- `HostedSession` handle (Step 4.6, not implemented)

#### D.2: Add explicit "future-planned" annotations

In `docs/podbot-design.md`, add a subsection under the public API
reference titled "Planned API surfaces" that lists each unimplemented
schema with its roadmap reference:

```markdown
### Planned API surfaces

The following API surfaces are documented in the design but not yet
implemented. They will be introduced in the referenced roadmap steps:

| Surface               | Roadmap step | Description                |
| --------------------- | ------------ | -------------------------- |
| `LaunchRequest/Plan`  | Step 4.5     | Normalized launch contract |
| Hook models           | Step 4.9     | Hook execution protocol    |
| Prompt/bundle schemas | Step 4.8     | Prompt validation surface  |
| MCP wire models       | Step 4.7     | MCP wire provisioning      |
| `HostedSession`       | Step 4.6     | Hosted session handle      |

Library consumers should not depend on these surfaces until their
roadmap steps are complete.
```

#### D.3: Verify existing MCP config types are documented

The `McpConfig`, `McpBindStrategy`, `McpAuthTokenPolicy`, and
`McpAllowedOriginPolicy` types are already public in both
`podbot::config` and `podbot::engine`. Verify they are included in the
public API reference table from Stage A.2.

**Stage D gate**: `make markdownlint` (documentation only).

### Stage E: Add integration tests for library embedding

**Goal:** Add integration tests that exercise Podbot as a library from a
host-style call path, proving the public API surface is self-contained.

#### E.1: Create `tests/library_embedding.rs`

An integration test that demonstrates library embedding without CLI
types. It should:

1. Construct `ConfigLoadOptions` and `ConfigOverrides` manually.
2. Call `load_config_with_env` with a mock environment.
3. Construct an `ExecParams` with a mock `ContainerExecClient`.
4. Call `podbot::api::exec()` and verify the `CommandOutcome`.
5. Call stub orchestration functions and verify outcomes.
6. Verify error types are matchable (`PodbotError` variants).

This test file proves that a host application can use the library
without importing `podbot::cli` or depending on `clap`.

#### E.2: Create BDD feature file `tests/features/library_boundary.feature`

```gherkin
Feature: Library boundary stability

  Podbot can be embedded as a Rust library dependency with
  documented, semantic APIs and no CLI coupling requirements.

  Scenario: Library consumer loads configuration without CLI types
    Given a mock environment with engine socket set
    And explicit load options without config discovery
    When the library configuration loader is called
    Then a valid AppConfig is returned
    And the engine socket matches the override

  Scenario: Library consumer executes a command via the API
    Given a mock container engine client
    And exec parameters for an attached echo command
    When the library exec function is called
    Then the outcome is success

  Scenario: Library consumer receives semantic error for missing
  config
    Given a mock environment without required fields
    And explicit load options requiring a config file
    When the library configuration loader is called
    Then the error is a ConfigError variant

  Scenario: Library consumer receives semantic error for exec
  failure
    Given a mock container engine client that fails on create exec
    And exec parameters for an attached echo command
    When the library exec function is called
    Then the error is a ContainerError variant

  Scenario: Stub orchestration functions return success
    When each stub orchestration function is called
    Then all outcomes are success
```

#### E.3: Create BDD helpers

Create `tests/bdd_library_boundary.rs` and
`tests/bdd_library_boundary_helpers/` following the established pattern:

- `mod.rs` — re-exports with `#[expect(unused_imports)]`
- `state.rs` — `LibraryBoundaryState` with `Slot<T>` fields
- `steps.rs` — given/when step functions
- `assertions.rs` — then assertion functions

The steps should exercise:

- `podbot::config::load_config_with_env` with `mockable::MockEnv`
- `podbot::api::exec` with a `mockall`-generated mock client
- `podbot::api::{run_agent, list_containers, stop_container,
  run_token_daemon}` stubs
- `podbot::error::{PodbotError, ConfigError, ContainerError}` matching

#### E.4: Add unit tests in `src/api/tests.rs`

Add additional unit tests for error contract verification:

- Test that `exec()` with an empty command returns
  `ConfigError::MissingRequired`.
- Test that `exec()` with a failing mock client returns a
  `ContainerError` variant.

#### E.5: Add unit test for `PodbotError` contract

In `src/error.rs` tests, add:

```rust
#[rstest]
fn podbot_error_implements_std_error() {
    fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}
    assert_error::<PodbotError>();
}
```

**Stage E gate**: `make check-fmt && make lint && make test`.

### Stage F: Documentation and roadmap updates

#### F.1: Update `docs/podbot-design.md`

1. Add the "Public library API reference" section (from Stage A.2).
2. Add the "Planned API surfaces" subsection (from Stage D.2).
3. Update the module structure diagram to show the `cli` feature gate.

#### F.2: Update `docs/users-guide.md`

1. Add a "Library embedding" section documenting:
   - The `cli` Cargo feature and how to opt out of it.
   - Instructions for embedding Podbot as a library dependency.
   - Which modules are stable and which are internal.
2. Update the "Library API" section to reference the feature gate.

#### F.3: Update `docs/podbot-roadmap.md`

Mark Step 5.3 tasks as done:

```markdown
- [x] Document supported public modules and request/response types.
- [x] Ensure public APIs use semantic errors (`PodbotError`) and avoid
  opaque `eyre` types.
- [x] Gate CLI-only dependencies and code paths behind a binary or
  feature boundary.
- [x] Reconcile the public hook and validation schemas with the
  documented integration contract before stabilizing them.
- [x] Add integration tests that embed Podbot as a library from a
  host-style call path.
```

#### F.4: Save execution plan

Update this document's status to COMPLETE.

**Stage F gate**: `make markdownlint` and full quality gates.

### Stage G: Final quality gate verification

Run the full quality gate:

```bash
set -o pipefail
make check-fmt 2>&1 | tee /tmp/check-fmt-5-3-1.log
make lint 2>&1 | tee /tmp/lint-5-3-1.log
make test 2>&1 | tee /tmp/test-5-3-1.log
cargo check --no-default-features 2>&1 | tee /tmp/no-default-5-3-1.log
```

Verify:

- All pre-existing tests pass unchanged.
- New unit tests pass.
- New BDD scenarios pass.
- New integration tests pass.
- Library compiles without CLI feature.

## Interfaces and dependencies

### Modified public API

In `src/lib.rs`:

```rust
// Before:
pub mod cli;

// After:
#[cfg(feature = "cli")]
pub mod cli;
```

### New Cargo features

```toml
[features]
default = ["cli"]
cli = ["dep:clap"]
```

### New test files

| File                                                    | Purpose                            |
| ------------------------------------------------------- | ---------------------------------- |
| `tests/library_embedding.rs`                            | Integration test for lib embedding |
| `tests/features/library_boundary.feature`               | BDD scenarios                      |
| `tests/bdd_library_boundary.rs`                         | Scenario bindings                  |
| `tests/bdd_library_boundary_helpers/mod.rs`             | Re-exports                         |
| `tests/bdd_library_boundary_helpers/state.rs`           | State + fixture                    |
| `tests/bdd_library_boundary_helpers/steps.rs`           | Given/when steps                   |
| `tests/bdd_library_boundary_helpers/assertions.rs`      | Then assertions                    |

### Files to modify

| File                      | Change                                    |
| ------------------------- | ----------------------------------------- |
| `Cargo.toml`              | Add `[features]`, make `clap` optional    |
| `src/lib.rs`              | Gate `cli` behind feature; update docs    |
| `src/error.rs`            | Add error contract tests                  |
| `src/api/tests.rs`        | Add error path unit tests                 |
| `docs/podbot-design.md`   | Add public API reference and planned APIs |
| `docs/users-guide.md`     | Add library embedding section             |
| `docs/podbot-roadmap.md`  | Mark Step 5.3 tasks as done               |

### Consumed (not modified) dependencies

- `podbot::api` — orchestration functions
- `podbot::config` — configuration types and loaders
- `podbot::engine` — container engine types and traits
- `podbot::error` — semantic error hierarchy
- `mockable` — environment abstraction for tests
- `mockall` — mock generation for tests
- `rstest` / `rstest-bdd` — test frameworks

No new external crate dependencies are required.

## Validation and acceptance

Done means:

1. `make check-fmt`, `make lint`, `make test` all pass.
2. `cargo check --no-default-features` succeeds.
3. New integration test `tests/library_embedding.rs` passes, proving
   host-style library embedding works without CLI types.
4. New BDD scenarios in `tests/features/library_boundary.feature` pass.
5. `docs/podbot-design.md` contains a "Public library API reference"
   section documenting all stable modules.
6. `docs/podbot-design.md` contains a "Planned API surfaces" subsection
   documenting unimplemented schema surfaces.
7. `docs/users-guide.md` documents the `cli` feature flag and library
   embedding.
8. `docs/podbot-roadmap.md` Step 5.3 tasks are all marked done.
9. No `eyre` types appear in any public library function signature.
10. The `cli` module is gated behind the `cli` Cargo feature.

Quality method:

```bash
set -o pipefail
make check-fmt 2>&1 | tee /tmp/check-fmt-5-3-1.log
make lint 2>&1 | tee /tmp/lint-5-3-1.log
make test 2>&1 | tee /tmp/test-5-3-1.log
cargo check --no-default-features 2>&1 | tee /tmp/no-default-5-3-1.log
```

## Idempotence and recovery

All stages are additive and can be rerun safely. If partial edits leave
the tree failing, revert only incomplete hunks and replay the current
stage. `cargo clean -p podbot` may be needed after modifying feature
files (`rstest-bdd` reads them at compile time). Gate logs stored under
`/tmp` with unique names per run.

## Implementation order

1. `docs/podbot-design.md` — add public API reference (Stage A.2)
2. `src/error.rs` — add error contract tests (Stage B.2)
3. `Cargo.toml` — add features section (Stage C.1)
4. `src/lib.rs` — gate `cli` module (Stage C.2)
5. Verify builds (Stage C.3, C.4)
6. `src/lib.rs` — add feature gate test (Stage C.6)
7. Gate check (Stage C)
8. `docs/podbot-design.md` — add planned API surfaces (Stage D.2)
9. `tests/library_embedding.rs` — integration test (Stage E.1)
10. `tests/features/library_boundary.feature` — BDD feature (Stage E.2)
11. `tests/bdd_library_boundary_helpers/state.rs` (Stage E.3)
12. `tests/bdd_library_boundary_helpers/steps.rs` (Stage E.3)
13. `tests/bdd_library_boundary_helpers/assertions.rs` (Stage E.3)
14. `tests/bdd_library_boundary_helpers/mod.rs` (Stage E.3)
15. `tests/bdd_library_boundary.rs` (Stage E.3)
16. `src/api/tests.rs` — error path tests (Stage E.4)
17. Gate check (Stage E)
18. `docs/users-guide.md` — library embedding section (Stage F.2)
19. `docs/podbot-roadmap.md` — mark Step 5.3 done (Stage F.3)
20. Final gate check (Stage G)
