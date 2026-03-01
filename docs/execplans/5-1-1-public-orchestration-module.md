# Extract command orchestration into a public `api/` library module

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose / big picture

After this change, external Rust applications can import `podbot::api` and call
orchestration functions (`exec`, `run_agent`, `stop_container`,
`list_containers`, `run_token_daemon`) without shelling out to the CLI binary.
The CLI binary (`src/main.rs`) becomes a thin adapter that parses arguments,
calls library orchestration, formats output, and converts outcomes to process
exit codes.

Observable success: `make check-fmt && make lint && make test` all pass. The
existing `podbot exec` behaviour is identical before and after (interactive
sessions, resize events, exit codes). A library consumer can write
`podbot::api::exec(...)` and receive a typed `CommandOutcome` without depending
on clap types or triggering stdout output. This is Step 5.1 of Phase 5 in the
roadmap (`docs/podbot-roadmap.md`).

## Constraints

Hard invariants that must hold throughout implementation. Violation requires
escalation, not workarounds.

- Files must be fewer than 400 lines each.
- Every module must begin with a `//!` module-level doc comment.
- en-GB-oxendict spelling ("-ize" / "-yse" / "-our") in all comments and
  documentation.
- No `unwrap()` or `expect()` in production code (clippy denies `unwrap_used`,
  `expect_used`).
- No `println!` or `eprintln!` in library code (clippy denies `print_stdout`,
  `print_stderr`). Library API functions must not print to stdout/stderr
  directly.
- Library API functions must not depend on clap types (`Cli`, `ExecArgs`,
  `RunArgs`, `StopArgs`, `TokenDaemonArgs`).
- Library API functions must not call `std::process::exit` or detect terminal
  state (`std::io::stdin().is_terminal()`).
- Library API functions return `podbot::error::Result<CommandOutcome>`.
- Existing public API signatures in `podbot::engine` and `podbot::config` must
  not change.
- No new external crate dependencies may be added.
- `rstest` for unit tests; `rstest-bdd` v0.5.0 for behavioural tests.
- BDD step function parameter names must match fixture names exactly.
- BDD feature files must use unquoted text for `{param}` captures.
- BDD tests must use `StepResult<T> = Result<T, String>` pattern (no
  `expect`/`panic`).
- `make check-fmt`, `make lint`, `make test` must pass before any commit.
- Commit messages use imperative mood; atomic commits; one logical unit per
  commit.

## Tolerances (exception triggers)

- **Scope**: if implementation requires changes to more than 15 files or 700
  net lines of code, stop and escalate.
- **Interface**: if any existing public API signature in `podbot::engine` or
  `podbot::config` must change, stop and escalate.
- **Dependencies**: if a new external crate dependency is required, stop and
  escalate.
- **Iterations**: if tests still fail after 3 focused fix attempts on a single
  issue, stop and escalate.
- **Ambiguity**: if multiple valid interpretations exist for how a stub
  orchestration function should behave, stop and present options.

## Risks

- Risk: BDD test scaffolding for orchestration may conflict with existing test
  binary names or fixture names. Severity: low Likelihood: low Mitigation: use
  distinct names prefixed with `orchestration_` for state types and fixture
  functions.

- Risk: the `CommandOutcome` type move could break clippy lint expectations in
  `main.rs` (the `#[expect(...)]` attributes on stub handlers). Severity: low
  Likelihood: high Mitigation: when stubs move to the library, `print_stdout`
  suppression stays in CLI; `unnecessary_wraps` moves to library stubs with
  FIXME issue link.

- Risk: `api::exec()` testability — if the function creates its own
  `EngineConnector` internally, mock injection is difficult for end-to-end BDD
  tests. Severity: medium Likelihood: high Mitigation: accept `connector: &C`
  where `C: ContainerExecClient` via `ExecParams`, following the project's DI
  convention (`docs/reliable-testing-in-rust-via-dependency-injection.md`). The
  CLI adapter resolves the engine socket and injects the connected client;
  tests supply a mock implementation.

## Progress

- [x] Stage A: Create `src/api/` module with `CommandOutcome`, exec
  extraction, stubs, unit tests, and lib.rs export.
- [x] Stage A: Update `src/main.rs` to become thin CLI adapter.
- [x] Stage A: Gate check (`make check-fmt`, `make lint`, `make test`).
- [x] Stage B: Create BDD feature file, test scaffolding, steps, assertions.
- [x] Stage B: Gate check (`make check-fmt`, `make lint`, `make test`).
- [x] Stage C: Update design doc, users' guide, roadmap; save execution plan.
- [x] Stage C: Gate check (full stack including markdownlint).
- [x] Stage D: Final verification and commit.

## Surprises & discoveries

- Surprise: clippy `too_many_arguments` triggered on the initial `exec()`
  function with 7 parameters. Resolution: introduced `ExecParams<'a, C>` struct
  (where `C: ContainerExecClient`) to group parameters, following the pattern
  from `bollard`.

- Surprise: clippy `missing_const_for_fn` triggered on stub functions instead
  of `unnecessary_wraps`. The stubs were trivial enough to be const-eligible.
  Resolution: changed `#[expect]` from `unnecessary_wraps` to
  `missing_const_for_fn` with `FIXME(#51)` annotation.

- Discovery: BDD step functions that don't use `?` trigger
  `unnecessary_wraps`. The existing test suites solve this with
  `#[expect(clippy::unnecessary_wraps,
  reason = "rstest-bdd step functions must return StepResult
  for consistency")]`.

## Decision log

- Decision: keep `normalize_process_exit_code` in `main.rs` rather than moving
  to the library. Rationale: this function converts `i64` to process exit codes
  (`i32` clamped to 0–255), which is purely a CLI concern. Library embedders
  may have different exit-code mapping requirements.

- Decision: `CommandOutcome` uses `i64` for exit codes (not `u8` or `i32`).
  Rationale: container engines report exit codes as `i64` (Bollard's
  `ExecInspectResponse.exit_code` is `Option<i64>`). Narrowing at the library
  level would lose information.

- Decision: `api::exec()` accepts `tty: bool` parameter rather than detecting
  terminals. Rationale: `std::io::stdin().is_terminal()` is a CLI concern.
  Library embedders may not have a terminal at all. Pushing terminal detection
  to the caller follows the design doc requirement.

- Decision: `api::exec()` accepts `connector: &C` where
  `C: ContainerExecClient` rather than creating an engine connection
  internally. Rationale: follows the project's DI convention documented in
  `docs/reliable-testing-in-rust-via-dependency-injection.md`. Makes the
  function testable with a mock `ContainerExecClient` without requiring a live
  engine socket. The CLI adapter handles socket resolution and connection.

- Decision: stub functions do not print to stdout/stderr.
  Rationale: the design doc says library APIs must not print directly. CLI
  output for stubs moves to the CLI adapter layer in `main.rs`.

- Decision: BDD exec-orchestration tests inject a mock
  `ContainerExecClient` via `ExecParams { connector: &client, ... }` to
  exercise the full `api::exec()` code path with controlled exit codes.
  Rationale: since `api::exec()` accepts a pre-connected client, tests can
  supply a `MockOrcExecClient` that returns configured exit codes. This
  validates the exit-code-to-`CommandOutcome` mapping through the public API
  without requiring a live engine socket.

## Outcomes & retrospective

All acceptance criteria met:

- `make check-fmt`, `make lint`, `make test` all pass (312 tests total).
- 8 new unit tests in `src/api/tests.rs` pass.
- 6 new BDD orchestration scenarios pass.
- All 306 pre-existing tests pass unchanged.
- `podbot::api::CommandOutcome` is importable from library consumers.
- `podbot::api::exec()` accepts only library types via `ExecParams`.
- `src/main.rs` no longer defines `CommandOutcome` locally.
- Design doc, users' guide, and roadmap updated.
- Execution plan saved to `docs/execplans/5-1-1-public-orchestration-module.md`.

Key deviation from plan: `exec()` uses `ExecParams<'a, C>` struct (where
`C: ContainerExecClient`) instead of 7 positional parameters, due to clippy
`too_many_arguments`. The struct accepts a pre-connected `connector` rather
than `config`/`env` fields, pushing socket resolution to the caller (CLI
adapter) and enabling direct mock injection in tests.

## Context and orientation

Podbot is a Rust application that runs AI coding agents (Claude Code, Codex) in
sandboxed containers. It provides two delivery surfaces: a CLI binary and a
Rust library. The project lives at `/home/user/project`.

Currently, the CLI binary (`src/main.rs`, 207 lines) contains a local
`CommandOutcome` enum and five handler functions. Only `exec_in_container` has
real business logic; the other four (`run_agent`, `run_token_daemon`,
`list_containers`, `stop_container`) are stubs that print placeholder messages.
The library (`src/lib.rs`, 24 lines) exports three modules: `config`, `engine`,
`error`.

The design document (`docs/podbot-design.md`) specifies an `api/` directory in
the module structure for orchestration functions and requires that library APIs
use semantic errors, accept only library-owned types, and never print or exit.

Key files:

- `src/main.rs` — CLI binary with `CommandOutcome` and 5 handler functions
- `src/lib.rs` — library entry point; exports `config`, `engine`, `error`
- `src/error.rs` — semantic error types (`PodbotError`, `ContainerError`, etc.)
- `src/engine/mod.rs` — re-exports from `connection/`
- `src/engine/connection/mod.rs` — `EngineConnector`, `SocketResolver`
- `src/engine/connection/exec/mod.rs` — `ExecRequest`, `ExecResult`,
  `ExecMode`, `ContainerExecClient` trait, `EngineConnector::exec()`
- `src/config/cli.rs` — clap argument types (`ExecArgs`, `RunArgs`, etc.)
- `src/config/types.rs` — `AppConfig` and nested config structs
- `tests/bdd_interactive_exec.rs` — existing BDD test for exec behaviour
- `tests/bdd_interactive_exec_helpers/` — state, steps, assertions for exec BDD
- `docs/podbot-design.md` — architecture and dual-delivery model
- `docs/podbot-roadmap.md` — roadmap; Step 5.1 is the target

Existing patterns to follow:

- Module structure: `src/engine/mod.rs` re-exports from submodules. The `api/`
  module should follow this pattern.
- Dependency injection: `exec()` accepts a pre-connected
  `&impl ContainerExecClient` for engine access; other functions use
  `&impl mockable::Env` for environment variable access (see
  `docs/reliable-testing-in-rust-via-dependency-injection.md`).
- BDD tests: each suite has `tests/bdd_<name>.rs` (scenario bindings),
  `tests/bdd_<name>_helpers/` (mod.rs, state.rs, steps.rs, assertions.rs),
  `tests/features/<name>.feature`.
- Error handling: library returns `podbot::error::Result<T>`; CLI converts
  to `eyre::Report` at the boundary.

## Plan of work

### Stage A: Create `src/api/` module and update binary

This stage introduces the public orchestration module with `CommandOutcome`,
the extracted exec function, stub functions, unit tests, and updates the CLI
binary to delegate to the library.

#### A.1: Create `src/api/exec.rs`

Create the file containing the extracted exec orchestration logic. The function
accepts only library types and a pre-connected `ContainerExecClient` for
testability.

```rust
//! Container command execution orchestration.
//!
//! This module provides the library-facing exec orchestration function that
//! builds an exec request, runs it via an injected
//! [`ContainerExecClient`](crate::engine::ContainerExecClient), and returns
//! the command outcome. Terminal detection (whether stdin/stdout are TTYs)
//! and engine connection are the caller's responsibility.

use crate::engine::{ContainerExecClient, EngineConnector, ExecMode, ExecRequest};
use crate::error::Result as PodbotResult;

use super::CommandOutcome;

/// Parameters for executing a command in a running container.
pub struct ExecParams<'a, C: ContainerExecClient> {
    pub connector: &'a C,
    pub container: &'a str,
    pub command: Vec<String>,
    pub mode: ExecMode,
    pub tty: bool,
    pub runtime_handle: &'a tokio::runtime::Handle,
}

/// Execute a command in a running container.
///
/// Builds an exec request, runs it via the supplied `connector`, and
/// maps the exit code to a [`CommandOutcome`].
///
/// # Errors
///
/// Returns `PodbotError` variants:
/// - `ContainerError::ExecFailed` if command execution fails.
/// - `ConfigError::MissingRequired` if required fields are empty.
pub fn exec<C: ContainerExecClient>(
    params: ExecParams<'_, C>,
) -> PodbotResult<CommandOutcome> {
    let ExecParams {
        connector, container, command, mode, tty, runtime_handle,
    } = params;

    let request = ExecRequest::new(container, command, mode)?.with_tty(tty);
    let exec_result = EngineConnector::exec(runtime_handle, connector, &request)?;

    if exec_result.exit_code() == 0 {
        Ok(CommandOutcome::Success)
    } else {
        Ok(CommandOutcome::CommandExit {
            code: exec_result.exit_code(),
        })
    }
}
```

Key design points:

- Accepts `connector: &C` where `C: ContainerExecClient` — the CLI adapter
  connects via `EngineConnector::connect_with_fallback` and passes the
  resulting `Docker` client; tests supply a mock implementation.
- Accepts `tty: bool` — caller decides, not the library.
- Returns `PodbotResult<CommandOutcome>` — typed outcome, no printing.
- No clap types in the signature.
- Socket resolution is the caller's responsibility, not the library's.

#### A.2: Create `src/api/mod.rs`

Create the orchestration module hub with `CommandOutcome`, stub functions, and
re-exports.

```rust
//! Orchestration API for podbot commands.
//!
//! This module provides public orchestration functions for each podbot
//! command: `exec`, `run_agent`, `stop_container`, `list_containers`, and
//! `run_token_daemon`. These functions contain the business logic that was
//! previously embedded in the CLI binary, making it available to both the
//! CLI adapter and library embedders.
//!
//! All functions accept library-owned types (not clap types) and return
//! `podbot::error::Result<CommandOutcome>`. They do not print to
//! stdout/stderr or call `std::process::exit`.

mod exec;

pub use exec::exec;

use crate::config::AppConfig;
use crate::error::Result as PodbotResult;

/// Outcome of a podbot command.
///
/// Commands return either outright success or a command-specific exit code
/// that the CLI adapter maps to a process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcome {
    /// The command completed successfully (exit code 0).
    Success,
    /// The command completed but the underlying process exited with a
    /// non-zero code.
    CommandExit {
        /// The exit code reported by the container engine.
        code: i64,
    },
}

/// Run an AI agent in a sandboxed container.
///
/// Placeholder for the full orchestration flow defined in the design
/// document (steps 1 through 7).
///
/// # Errors
///
/// Will return errors when container orchestration is implemented.
#[expect(
    clippy::missing_const_for_fn,
    reason = "FIXME(https://github.com/leynos/podbot/issues/51): stub is const-eligible but will gain runtime logic"
)]
pub fn run_agent(_config: &AppConfig) -> PodbotResult<CommandOutcome> {
    Ok(CommandOutcome::Success)
}

/// List running podbot containers.
///
/// # Errors
///
/// Will return errors when container listing is implemented.
#[expect(
    clippy::missing_const_for_fn,
    reason = "FIXME(https://github.com/leynos/podbot/issues/51): stub is const-eligible but will gain runtime logic"
)]
pub fn list_containers() -> PodbotResult<CommandOutcome> {
    Ok(CommandOutcome::Success)
}

/// Stop a running container.
///
/// # Errors
///
/// Will return errors when container stop is implemented.
#[expect(
    clippy::missing_const_for_fn,
    reason = "FIXME(https://github.com/leynos/podbot/issues/51): stub is const-eligible but will gain runtime logic"
)]
pub fn stop_container(_container: &str) -> PodbotResult<CommandOutcome> {
    Ok(CommandOutcome::Success)
}

/// Run the token refresh daemon for a container.
///
/// # Errors
///
/// Will return errors when the token daemon is implemented.
#[expect(
    clippy::missing_const_for_fn,
    reason = "FIXME(https://github.com/leynos/podbot/issues/51): stub is const-eligible but will gain runtime logic"
)]
pub fn run_token_daemon(_container_id: &str) -> PodbotResult<CommandOutcome> {
    Ok(CommandOutcome::Success)
}

#[cfg(test)]
mod tests;
```

#### A.3: Create `src/api/tests.rs`

Unit tests for `CommandOutcome` type behaviour and stub functions using
`rstest`.

```rust
//! Unit tests for the orchestration API module.

use rstest::rstest;

use super::{CommandOutcome, list_containers, run_agent, run_token_daemon, stop_container};
use crate::config::AppConfig;

#[rstest]
fn command_outcome_success_equals_itself() {
    assert_eq!(CommandOutcome::Success, CommandOutcome::Success);
}

#[rstest]
fn command_outcome_exit_preserves_code() {
    let outcome = CommandOutcome::CommandExit { code: 42 };
    assert_eq!(outcome, CommandOutcome::CommandExit { code: 42 });
}

#[rstest]
fn command_outcome_success_differs_from_exit_zero() {
    assert_ne!(
        CommandOutcome::Success,
        CommandOutcome::CommandExit { code: 0 }
    );
}

#[rstest]
fn run_agent_stub_returns_success() {
    let config = AppConfig::default();
    let result = run_agent(&config);
    assert!(result.is_ok());
    assert_eq!(result.ok(), Some(CommandOutcome::Success));
}

#[rstest]
fn list_containers_stub_returns_success() {
    let result = list_containers();
    assert!(result.is_ok());
    assert_eq!(result.ok(), Some(CommandOutcome::Success));
}

#[rstest]
fn stop_container_stub_returns_success() {
    let result = stop_container("test-container");
    assert!(result.is_ok());
    assert_eq!(result.ok(), Some(CommandOutcome::Success));
}

#[rstest]
fn run_token_daemon_stub_returns_success() {
    let result = run_token_daemon("test-container-id");
    assert!(result.is_ok());
    assert_eq!(result.ok(), Some(CommandOutcome::Success));
}
```

#### A.4: Update `src/lib.rs`

Add `pub mod api;` and update the module list in the doc comment.

Change the `# Modules` section to include:

```plaintext
//! - [`api`]: Orchestration API for run, exec, stop, ps, and token daemon commands
```

And add `pub mod api;` before the existing `pub mod config;`.

#### A.5: Update `src/main.rs`

Rewrite as a thin CLI adapter. Key changes:

1. Remove the local `CommandOutcome` enum (lines 66–69).
2. Import `podbot::api::CommandOutcome` instead.
3. Replace `exec_in_container` with a thin wrapper that resolves the engine
   socket, connects via `EngineConnector::connect_with_fallback`, and calls
   `podbot::api::exec()` with the connected client and TTY detection result.
4. Replace stub handlers with thin wrappers that call
   `podbot::api::{run_agent, list_containers, stop_container, run_token_daemon}`
    and add CLI-specific `println!` output.
5. Keep `normalize_process_exit_code` and its tests in `main.rs` (CLI-specific).
6. Keep `create_runtime` in `main.rs`.

The `run()` dispatch function becomes:

```rust
fn run(
    cli: &Cli,
    config: &AppConfig,
    runtime_handle: &tokio::runtime::Handle,
) -> PodbotResult<CommandOutcome> {
    match &cli.command {
        Commands::Run(_args) => run_agent_cli(config),
        Commands::TokenDaemon(args) => run_token_daemon_cli(args),
        Commands::Ps => list_containers_cli(),
        Commands::Stop(args) => stop_container_cli(args),
        Commands::Exec(args) => exec_in_container_cli(config, args, runtime_handle),
    }
}
```

The exec CLI handler performs terminal detection, connects to the engine, and
delegates to the library:

```rust
fn exec_in_container_cli(
    config: &AppConfig,
    args: &ExecArgs,
    runtime_handle: &tokio::runtime::Handle,
) -> PodbotResult<CommandOutcome> {
    let mode = if args.detach {
        ExecMode::Detached
    } else {
        ExecMode::Attached
    };
    let tty = !args.detach
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal();
    let env = mockable::DefaultEnv::new();
    let resolver = SocketResolver::new(&env);
    let docker = EngineConnector::connect_with_fallback(
        config.engine_socket.as_deref(), &resolver,
    )?;

    podbot::api::exec(ExecParams {
        connector: &docker,
        container: &args.container,
        command: args.command.clone(),
        mode,
        tty,
        runtime_handle,
    })
}
```

Stub CLI handlers add user-facing output:

```rust
#[expect(clippy::print_stdout, reason = "CLI output is the intended behaviour")]
fn run_agent_cli(config: &AppConfig) -> PodbotResult<CommandOutcome> {
    let result = podbot::api::run_agent(config)?;
    println!("Container orchestration not yet implemented.");
    Ok(result)
}
```

Same pattern for `list_containers_cli`, `stop_container_cli`,
`run_token_daemon_cli` — each calls the library stub, prints a CLI message, and
returns the outcome.

**Stage A gate**: run `make check-fmt && make lint && make test`. All existing
tests must pass. New unit tests in `src/api/tests.rs` must pass.

### Stage B: BDD behavioural tests

#### B.1: Create `tests/features/orchestration.feature`

```gherkin
Feature: Command orchestration

  The podbot library provides orchestration functions that execute
  commands in containers and return typed outcomes without printing or
  exiting.

  Scenario: Exec orchestration returns success for zero exit code
    Given a connected container engine
    And exec mode is attached
    And tty is enabled
    And the command is echo hello
    And the daemon reports exit code 0
    When exec orchestration is called
    Then the outcome is success

  Scenario: Exec orchestration returns command exit for non-zero exit code
    Given a connected container engine
    And exec mode is detached
    And the command is sh -c exit 7
    And the daemon reports exit code 7
    When exec orchestration is called
    Then the outcome is command exit with code 7

  Scenario: Exec orchestration propagates connection errors
    Given the container engine socket is missing
    When exec orchestration is called
    Then the outcome is a connection error

  Scenario: Run stub returns success
    When run orchestration is called
    Then the outcome is success

  Scenario: Stop stub returns success
    When stop orchestration is called with container test-ctr
    Then the outcome is success

  Scenario: List containers stub returns success
    When list containers orchestration is called
    Then the outcome is success

  Scenario: Token daemon stub returns success
    When token daemon orchestration is called with container test-ctr
    Then the outcome is success
```

#### B.2: Create `tests/bdd_orchestration_helpers/state.rs`

State struct with `Slot<T>` fields for mode, tty, command, exit code, engine
availability, and result. Fixture function `orchestration_state()` returns a
default-initialised state.

Use `OrchestrationResult` enum with success and error variants
(`Ok(CommandOutcome)` and `Err(String)`) for capturing outcomes.

#### B.3: Create `tests/bdd_orchestration_helpers/steps.rs`

Given/when/then step functions. The `when exec orchestration is called` step
exercises the exec code path by injecting a `MockOrcExecClient` via
`ExecParams { connector: &client, ... }`, reusing the mock pattern from
`bdd_interactive_exec_helpers/steps.rs`. The mock client is configured with the
expected exit code, and the step calls `podbot::api::exec()` through the public
API. For stub scenarios, it calls the library stubs directly.

All step functions use `StepResult<T> = Result<T, String>` and match fixture
parameter names exactly (`orchestration_state: &OrchestrationState`).

#### B.4: Create `tests/bdd_orchestration_helpers/assertions.rs`

Assertion helpers for outcome verification following the `StepResult` pattern.

#### B.5: Create `tests/bdd_orchestration_helpers/mod.rs`

Re-export hub with `#[expect(unused_imports)]` on step and assertion imports
(matching the existing pattern from `bdd_interactive_exec_helpers`).

#### B.6: Create `tests/bdd_orchestration.rs`

Scenario bindings using `#[scenario]` macros, one function per scenario, each
accepting `orchestration_state: OrchestrationState`.

**Stage B gate**: run `make check-fmt && make lint && make test`. All existing
and new tests must pass.

### Stage C: Documentation updates

#### C.1: Update `docs/podbot-design.md`

In the "Module structure" section, add detail about `api/` contents:

```plaintext
├── api/                # Orchestration API: run, exec, stop, ps, token daemon
│   ├── mod.rs          # CommandOutcome type, stub functions, re-exports
│   ├── exec.rs         # Exec orchestration (extracted from main.rs)
│   └── tests.rs        # Unit tests for API module
```

In the "Library API boundary requirements" section, add a bullet:

- `CommandOutcome` is the typed return value from all orchestration
  functions. `Success` means exit code 0; `CommandExit { code }` carries the
  non-zero exit code for the CLI to map to a process exit code.

#### C.2: Update `docs/users-guide.md`

Add a "Library API" section documenting the available orchestration functions
for library embedders. No user-facing CLI behaviour changes.

#### C.3: Update `docs/podbot-roadmap.md`

Mark Step 5.1 first task as done:

```markdown
- [x] Introduce a public orchestration module for run, exec, stop, ps, and
  token daemon operations.
```

#### C.4: Save execution plan

Write the final execution plan to
`docs/execplans/5-1-1-public-orchestration-module.md`.

**Stage C gate**: run full gate stack including
`MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint`.

### Stage D: Final verification

Run the full quality gate:

```bash
set -o pipefail
make check-fmt 2>&1 | tee /tmp/check-fmt-5-1-1.log
make lint 2>&1 | tee /tmp/lint-5-1-1.log
make test 2>&1 | tee /tmp/test-5-1-1.log
```

Verify all pre-existing BDD tests pass unchanged. Verify new tests pass. Commit
with descriptive message.

## Interfaces and dependencies

### New public types

In `src/api/mod.rs`:

```rust
/// Outcome of a podbot command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcome {
    Success,
    CommandExit { code: i64 },
}
```

### New public functions

In `src/api/mod.rs`:

```rust
pub fn run_agent(config: &AppConfig) -> PodbotResult<CommandOutcome>;
pub fn list_containers() -> PodbotResult<CommandOutcome>;
pub fn stop_container(container: &str) -> PodbotResult<CommandOutcome>;
pub fn run_token_daemon(container_id: &str) -> PodbotResult<CommandOutcome>;
```

In `src/api/exec.rs`:

```rust
pub struct ExecParams<'a, C: ContainerExecClient> {
    pub connector: &'a C,
    pub container: &'a str,
    pub command: Vec<String>,
    pub mode: ExecMode,
    pub tty: bool,
    pub runtime_handle: &'a tokio::runtime::Handle,
}

pub fn exec<C: ContainerExecClient>(
    params: ExecParams<'_, C>,
) -> PodbotResult<CommandOutcome>;
```

### Consumed (not modified) dependencies

- `podbot::engine::ContainerExecClient` — trait for engine operations
- `podbot::engine::EngineConnector` — `connect_with_fallback()`, `exec()`
- `podbot::engine::SocketResolver` — env var resolution (used by CLI adapter)
- `podbot::engine::ExecRequest`, `ExecMode`, `ExecResult` — request/response
- `podbot::error::PodbotError`, `podbot::error::Result` — error handling

No new external crate dependencies are required.

## Files to create

| File                                                  | Purpose                                      |
| ----------------------------------------------------- | -------------------------------------------- |
| `src/api/mod.rs`                                      | `CommandOutcome`, stub functions, re-exports |
| `src/api/exec.rs`                                     | Exec orchestration extracted from `main.rs`  |
| `src/api/tests.rs`                                    | Unit tests for `CommandOutcome` and stubs    |
| `tests/features/orchestration.feature`                | BDD scenarios                                |
| `tests/bdd_orchestration.rs`                          | Scenario bindings                            |
| `tests/bdd_orchestration_helpers/mod.rs`              | Re-exports                                   |
| `tests/bdd_orchestration_helpers/state.rs`            | State + fixture                              |
| `tests/bdd_orchestration_helpers/steps.rs`            | Given/when/then                              |
| `tests/bdd_orchestration_helpers/assertions.rs`       | Assertion helpers                            |
| `docs/execplans/5-1-1-public-orchestration-module.md` | This plan                                    |

## Files to modify

| File                     | Change                                                                        |
| ------------------------ | ----------------------------------------------------------------------------- |
| `src/lib.rs`             | Add `pub mod api;` and update module doc                                      |
| `src/main.rs`            | Remove `CommandOutcome`, replace handlers with thin calls to `podbot::api::*` |
| `docs/podbot-design.md`  | Add `api/` module detail                                                      |
| `docs/users-guide.md`    | Add Library API section                                                       |
| `docs/podbot-roadmap.md` | Mark Step 5.1 first task done                                                 |

## Validation and acceptance

Quality criteria:

- `make check-fmt` passes (cargo fmt workspace check).
- `make lint` passes (clippy with all warnings denied, cargo doc generation).
- `make test` passes (all existing and new tests).
- New unit test `api::tests::command_outcome_success_equals_itself` passes.
- New unit test `api::tests::run_agent_stub_returns_success` passes.
- BDD scenario "Exec orchestration returns success for zero exit code" passes.
- BDD scenario "Exec orchestration returns command exit for non-zero exit
  code" passes.
- BDD scenario "Run stub returns success" passes.
- All pre-existing BDD scenarios pass unchanged.
- `podbot::api::CommandOutcome` is importable from library consumers.
- `podbot::api::exec()` is callable with library types only (no clap).
- `src/main.rs` no longer defines `CommandOutcome` locally.

Quality method:

```bash
set -o pipefail
make check-fmt 2>&1 | tee /tmp/check-fmt-5-1-1.log
make lint 2>&1 | tee /tmp/lint-5-1-1.log
make test 2>&1 | tee /tmp/test-5-1-1.log
```

## Idempotence and recovery

All stages are additive and can be rerun safely. If partial edits leave the
tree failing, revert only incomplete hunks and replay the current stage.
`cargo clean -p podbot` may be needed after modifying feature files (rstest-bdd
reads them at compile time). Gate logs stored under `/tmp` with unique names
per run.

## Implementation order

1. `src/api/exec.rs` — core extracted logic
2. `src/api/mod.rs` — `CommandOutcome` + stubs + re-exports
3. `src/api/tests.rs` — unit tests
4. `src/lib.rs` — add `pub mod api;`
5. `src/main.rs` — thin adapter rewrite
6. Gate check (Stage A)
7. `tests/features/orchestration.feature`
8. `tests/bdd_orchestration_helpers/state.rs`
9. `tests/bdd_orchestration_helpers/steps.rs`
10. `tests/bdd_orchestration_helpers/assertions.rs`
11. `tests/bdd_orchestration_helpers/mod.rs`
12. `tests/bdd_orchestration.rs`
13. Gate check (Stage B)
14. `docs/podbot-design.md` update
15. `docs/users-guide.md` update
16. `docs/podbot-roadmap.md` update
17. `docs/execplans/5-1-1-public-orchestration-module.md`
18. Final gate check (Stage D)
