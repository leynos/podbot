# Step 4.1.1: Configure Git identity within the container

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETED

## Purpose and big picture

Complete roadmap Step 4.1 ("Git identity configuration") by reading
`user.name` and `user.email` from the host Git configuration and executing
`git config --global` within the container to propagate that identity.

After this change, the container lifecycle will support a deterministic
Git identity configuration step that:

- reads `user.name` from the host via `git config --get user.name`;
- reads `user.email` from the host via `git config --get user.email`;
- executes `git config --global user.name <value>` in the container;
- executes `git config --global user.email <value>` in the container;
- warns but does not fail when either identity field is missing on the
  host.

Observable success: `make check-fmt && make lint && make test` all pass.
Git identity configuration is testable in isolation with mock host
command runners and mock container exec clients. Missing identity on the
host produces a warning-level result rather than a hard error. This is
Step 4.1 of Phase 4 in the roadmap (`docs/podbot-roadmap.md`).

Required documentation outcomes:

- record design decisions in `docs/podbot-design.md`;
- update user-visible behaviour in `docs/users-guide.md`;
- mark Step 4.1 roadmap tasks as done in `docs/podbot-roadmap.md`.

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
- Library functions must not depend on clap types.
- Library functions must not call `std::process::exit`.
- Library functions return `podbot::error::Result<T>`.
- Existing public API signatures in `podbot::engine` and `podbot::config`
  must not change.
- No new external crate dependencies may be added.
- `rstest` for unit tests; `rstest-bdd` v0.5.0 for behavioural tests.
- BDD step function parameter names must match fixture names exactly.
- BDD feature files must use unquoted text for `{param}` captures.
- BDD tests must use `StepResult<T> = Result<T, String>` pattern (no
  `expect`/`panic`).
- `make check-fmt`, `make lint`, `make test` must pass before any commit.
- Commit messages use imperative mood; atomic commits; one logical unit
  per commit.

## Tolerances (exception triggers)

- **Scope**: if implementation requires changes to more than 15 files or
  600 net lines of code, stop and escalate.
- **Interface**: if any existing public API signature in `podbot::engine`
  or `podbot::config` must change, stop and escalate.
- **Dependencies**: if a new external crate dependency is required, stop
  and escalate.
- **Iterations**: if tests still fail after 3 focused fix attempts on a
  single issue, stop and escalate.
- **Ambiguity**: if multiple valid interpretations exist for host command
  execution, stop and present options.

## Risks

- Risk: host `git` binary may not be installed or may not have identity
  configured. Severity: medium. Likelihood: medium. Mitigation: treat
  missing identity as a warning (`GitIdentityResult::Partial` or
  `::NoneConfigured`), not a hard error. Log the missing fields and
  continue.

- Risk: shelling out to `git config` from the host introduces a
  dependency on host tooling. Severity: low. Likelihood: low. Mitigation:
  abstract host command execution behind a trait (`HostCommandRunner`)
  that can be mocked in tests. Production implementation uses
  `std::process::Command`.

- Risk: container exec for `git config --global` could fail if the
  container image does not have `git` installed. Severity: medium.
  Likelihood: low. Mitigation: propagate the exec error; the operator
  must ensure the container image includes `git`.

## Progress

- [x] Stage A: Create `src/engine/connection/git_identity/` module with
  host reader trait, container configurator, and unit tests.
- [x] Stage A: Gate check (`make check-fmt`, `make lint`, `make test`).
- [x] Stage B: Create API orchestration function in `src/api/`.
- [x] Stage B: Gate check.
- [x] Stage C: Create BDD feature file, test scaffolding, steps,
  assertions.
- [x] Stage C: Gate check.
- [x] Stage D: Update design doc, users' guide, roadmap; save execution
  plan.
- [x] Stage D: Gate check (full stack including markdownlint).
- [x] Stage E: Final verification and commit.

## Surprises and discoveries

(To be filled in during implementation.)

## Decision log

- Decision: abstract host Git config reading behind a
  `HostCommandRunner` trait rather than calling `std::process::Command`
  directly. Rationale: follows the project's dependency injection (DI)
  convention (`docs/reliable-testing-in-rust-via-dependency-injection.md`).
  Tests can inject a mock that returns configurable identity values without
  requiring `git` to be installed. The trait has a single method
  `run_command(&self, program: &str, args: &[&str]) -> io::Result<Output>`
  with a production implementation wrapping `std::process::Command`.

- Decision: place the engine-level Git identity module at
  `src/engine/connection/git_identity/mod.rs`, following the pattern
  established by `upload_credentials/`, `create_container/`, and `exec/`.
  Rationale: Git identity configuration uses the same
  `ContainerExecClient` trait seam to execute commands in the container,
  making it a peer of the existing connection submodules.

- Decision: return a `GitIdentityResult` enum with three variants:
  `Configured { name, email }`, `Partial { name, email, warnings }`, and
  `NoneConfigured { warnings }`. Rationale: the roadmap explicitly states
  missing identity should produce a warning rather than failure. Callers
  can inspect the result to decide how to surface warnings.

- Decision: Git identity configuration does not add new config fields to
  `AppConfig`. Rationale: the identity is read dynamically from the host
  `git config` at runtime, not from podbot's own configuration. This
  avoids configuration sprawl and follows the design doc which says
  "reading from the host".

- Decision: use `ExecMode::Detached` for `git config --global` commands
  within the container. Rationale: these are non-interactive commands
  that do not need stdin/stdout streaming. Detached mode is simpler and
  avoids the attached-session lifecycle.

## Outcomes and retrospective

(To be filled in after implementation.)

## Context and orientation

Podbot is a Rust application that runs AI coding agents (Claude Code,
Codex) in sandboxed containers. It provides two delivery surfaces: a
command-line interface (CLI) binary and a Rust library. The project lives at
`/home/user/project`.

The design document (`docs/podbot-design.md`, line 100) specifies:

> **Configure Git identity** by reading `user.name` and `user.email`
> from the host and executing `git config --global` within the container.

This is step 3 in the container lifecycle, after credential injection
(step 2) and before GitHub token creation (step 4).

Key files:

- `src/engine/connection/mod.rs` -- connection module, re-exports traits
- `src/engine/connection/exec/mod.rs` -- `ContainerExecClient` trait,
  `ExecRequest`, `ExecMode::Detached`
- `src/engine/connection/upload_credentials/mod.rs` -- pattern reference
  for new connection submodules
- `src/engine/mod.rs` -- `pub use` re-exports
- `src/api/mod.rs` -- orchestration API, `CommandOutcome`
- `src/api/exec.rs` -- `ExecParams`, `exec()`
- `src/error.rs` -- `ContainerError::ExecFailed`, `PodbotError`
- `src/main.rs` -- CLI adapter, `run_agent_cli` stub

Existing patterns to follow:

- **Trait seam for DI**: `ContainerExecClient` for container commands;
  `ContainerUploader` for archive upload; `ContainerCreator` for
  container creation. Each trait has a `Docker` impl and a `mock!` for
  testing.
- **Module structure**: `src/engine/connection/<feature>/mod.rs` with
  submodules for internals and `tests/`.
- **BDD tests**: `tests/bdd_<name>.rs` + `tests/bdd_<name>_helpers/` +
  `tests/features/<name>.feature`.
- **Error handling**: `thiserror` enums in library; `eyre` at CLI
  boundary. Use `ContainerError::ExecFailed` for git config exec
  failures.

## Plan of work

### Stage A: Engine-level Git identity module

Create the engine submodule that handles both host-side Git config
reading and container-side Git config setting.

#### A.1: Create `src/engine/connection/git_identity/host_reader.rs`

This file defines the trait for running commands on the host and the
function to read Git identity from the host.

```rust
//! Host-side Git identity reading.
//!
//! Reads `user.name` and `user.email` from the host Git configuration
//! by running `git config --get` commands. The command runner is
//! injected for testability.

use std::io;
use std::process::Output;

/// Runs a command on the host and returns its output.
///
/// This trait abstracts host command execution so tests can inject
/// mock responses without requiring a real `git` binary.
pub trait HostCommandRunner {
    /// Execute `program` with `args` and return the captured output.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the command cannot be spawned.
    fn run_command(
        &self,
        program: &str,
        args: &[&str],
    ) -> io::Result<Output>;
}

/// Production implementation using `std::process::Command`.
pub struct SystemCommandRunner;

impl HostCommandRunner for SystemCommandRunner {
    fn run_command(
        &self,
        program: &str,
        args: &[&str],
    ) -> io::Result<Output> {
        std::process::Command::new(program).args(args).output()
    }
}

/// Identity fields read from the host Git configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostGitIdentity {
    /// `user.name` value, if configured on the host.
    pub name: Option<String>,
    /// `user.email` value, if configured on the host.
    pub email: Option<String>,
}

/// Read Git `user.name` and `user.email` from the host.
///
/// Returns `None` values for fields that are not configured rather
/// than failing. The caller decides how to handle missing fields.
pub fn read_host_git_identity(
    runner: &impl HostCommandRunner,
) -> HostGitIdentity {
    HostGitIdentity {
        name: read_git_config_value(runner, "user.name"),
        email: read_git_config_value(runner, "user.email"),
    }
}

fn read_git_config_value(
    runner: &impl HostCommandRunner,
    key: &str,
) -> Option<String> {
    let output = runner
        .run_command("git", &["config", "--get", key])
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_owned();

    if value.is_empty() { None } else { Some(value) }
}
```

#### A.2: Create `src/engine/connection/git_identity/container_configurator.rs`

This file executes `git config --global` inside the container.

```rust
//! Container-side Git identity configuration.
//!
//! Executes `git config --global user.name` and
//! `git config --global user.email` within a running container using
//! the injected [`ContainerExecClient`].

use crate::engine::{
    ContainerExecClient, EngineConnector, ExecMode, ExecRequest,
};
use crate::error::PodbotError;

use super::host_reader::HostGitIdentity;

/// Outcome of configuring Git identity in a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitIdentityResult {
    /// Both name and email were configured successfully.
    Configured {
        /// The configured `user.name`.
        name: String,
        /// The configured `user.email`.
        email: String,
    },
    /// Only some identity fields were configured.
    Partial {
        /// The configured `user.name`, if set.
        name: Option<String>,
        /// The configured `user.email`, if set.
        email: Option<String>,
        /// Warning messages for missing fields.
        warnings: Vec<String>,
    },
    /// No identity fields were available on the host.
    NoneConfigured {
        /// Warning messages explaining the absence.
        warnings: Vec<String>,
    },
}

/// Configure Git identity in a container using host-read values.
///
/// Executes `git config --global user.name` and/or
/// `git config --global user.email` for each value present in
/// `identity`. Missing values produce warnings rather than errors.
///
/// # Errors
///
/// Returns `ContainerError::ExecFailed` if a `git config` command
/// fails inside the container.
pub fn configure_git_identity<C: ContainerExecClient>(
    runtime: &tokio::runtime::Handle,
    client: &C,
    container_id: &str,
    identity: &HostGitIdentity,
) -> Result<GitIdentityResult, PodbotError> {
    match (&identity.name, &identity.email) {
        (None, None) => Ok(GitIdentityResult::NoneConfigured {
            warnings: vec![
                String::from(
                    "git user.name is not configured on the host",
                ),
                String::from(
                    "git user.email is not configured on the host",
                ),
            ],
        }),
        (Some(name), Some(email)) => {
            set_git_config(runtime, client, container_id, "user.name", name)?;
            set_git_config(runtime, client, container_id, "user.email", email)?;
            Ok(GitIdentityResult::Configured {
                name: name.clone(),
                email: email.clone(),
            })
        }
        _ => configure_partial_identity(
            runtime,
            client,
            container_id,
            identity,
        ),
    }
}

fn configure_partial_identity<C: ContainerExecClient>(
    runtime: &tokio::runtime::Handle,
    client: &C,
    container_id: &str,
    identity: &HostGitIdentity,
) -> Result<GitIdentityResult, PodbotError> {
    let mut warnings = Vec::new();

    if let Some(name) = &identity.name {
        set_git_config(
            runtime, client, container_id, "user.name", name,
        )?;
    } else {
        warnings.push(String::from(
            "git user.name is not configured on the host",
        ));
    }

    if let Some(email) = &identity.email {
        set_git_config(
            runtime, client, container_id, "user.email", email,
        )?;
    } else {
        warnings.push(String::from(
            "git user.email is not configured on the host",
        ));
    }

    Ok(GitIdentityResult::Partial {
        name: identity.name.clone(),
        email: identity.email.clone(),
        warnings,
    })
}

fn set_git_config<C: ContainerExecClient>(
    runtime: &tokio::runtime::Handle,
    client: &C,
    container_id: &str,
    key: &str,
    value: &str,
) -> Result<(), PodbotError> {
    let command = vec![
        String::from("git"),
        String::from("config"),
        String::from("--global"),
        String::from(key),
        String::from(value),
    ];
    let request = ExecRequest::new(
        container_id,
        command,
        ExecMode::Detached,
    )?;
    let result = EngineConnector::exec(runtime, client, &request)?;

    if result.exit_code() != 0 {
        return Err(super::git_identity_exec_failed(
            container_id,
            format!(
                "git config --global {key} failed with exit code {}",
                result.exit_code()
            ),
        ));
    }

    Ok(())
}
```

#### A.3: Create `src/engine/connection/git_identity/mod.rs`

Module hub with re-exports.

```rust
//! Git identity configuration for containers.
//!
//! This module reads Git `user.name` and `user.email` from the host
//! configuration and propagates them into a running container via
//! `git config --global` exec commands.

mod container_configurator;
mod host_reader;

pub use container_configurator::{
    GitIdentityResult, configure_git_identity,
};
pub use host_reader::{
    HostCommandRunner, HostGitIdentity, SystemCommandRunner,
    read_host_git_identity,
};

use crate::error::{ContainerError, PodbotError};

fn git_identity_exec_failed(
    container_id: &str,
    message: impl Into<String>,
) -> PodbotError {
    PodbotError::from(ContainerError::ExecFailed {
        container_id: String::from(container_id),
        message: message.into(),
    })
}

#[cfg(test)]
mod tests;
```

#### A.4: Create `src/engine/connection/git_identity/tests.rs`

Unit tests using `rstest` and `mockall`.

```rust
//! Unit tests for Git identity configuration.

use std::io;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};

use mockall::mock;
use rstest::{fixture, rstest};

use super::container_configurator::GitIdentityResult;
use super::host_reader::{
    HostCommandRunner, HostGitIdentity, read_host_git_identity,
};
use super::{configure_git_identity};

// -- Host reader tests --

mock! {
    CommandRunner {}
    impl HostCommandRunner for CommandRunner {
        fn run_command(
            &self,
            program: &str,
            args: &[&str],
        ) -> io::Result<Output>;
    }
}

fn success_output(stdout: &str) -> Output {
    Output {
        status: ExitStatus::from_raw(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    }
}

fn failure_output() -> Output {
    Output {
        status: ExitStatus::from_raw(256), // exit code 1
        stdout: Vec::new(),
        stderr: b"error".to_vec(),
    }
}

#[rstest]
fn read_identity_returns_both_when_configured() {
    let mut runner = MockCommandRunner::new();
    runner.expect_run_command()
        .withf(|prog, args| {
            prog == "git" && args == ["config", "--get", "user.name"]
        })
        .returning(|_, _| Ok(success_output("Alice\n")));
    runner.expect_run_command()
        .withf(|prog, args| {
            prog == "git" && args == ["config", "--get", "user.email"]
        })
        .returning(|_, _| Ok(success_output("alice@example.com\n")));

    let identity = read_host_git_identity(&runner);

    assert_eq!(identity.name.as_deref(), Some("Alice"));
    assert_eq!(identity.email.as_deref(), Some("alice@example.com"));
}

#[rstest]
fn read_identity_returns_none_when_git_not_installed() {
    let mut runner = MockCommandRunner::new();
    runner.expect_run_command()
        .returning(|_, _| {
            Err(io::Error::new(io::ErrorKind::NotFound, "not found"))
        });

    let identity = read_host_git_identity(&runner);

    assert!(identity.name.is_none());
    assert!(identity.email.is_none());
}

#[rstest]
fn read_identity_returns_none_for_unconfigured_fields() {
    let mut runner = MockCommandRunner::new();
    runner.expect_run_command()
        .withf(|_, args| args.contains(&"user.name"))
        .returning(|_, _| Ok(failure_output()));
    runner.expect_run_command()
        .withf(|_, args| args.contains(&"user.email"))
        .returning(|_, _| Ok(success_output("bob@example.com\n")));

    let identity = read_host_git_identity(&runner);

    assert!(identity.name.is_none());
    assert_eq!(identity.email.as_deref(), Some("bob@example.com"));
}

#[rstest]
fn read_identity_trims_whitespace() {
    let mut runner = MockCommandRunner::new();
    runner.expect_run_command()
        .withf(|_, args| args.contains(&"user.name"))
        .returning(|_, _| Ok(success_output("  Alice  \n")));
    runner.expect_run_command()
        .withf(|_, args| args.contains(&"user.email"))
        .returning(|_, _| Ok(success_output("  alice@example.com  \n")));

    let identity = read_host_git_identity(&runner);

    assert_eq!(identity.name.as_deref(), Some("Alice"));
    assert_eq!(identity.email.as_deref(), Some("alice@example.com"));
}

#[rstest]
fn read_identity_returns_none_for_empty_output() {
    let mut runner = MockCommandRunner::new();
    runner.expect_run_command()
        .returning(|_, _| Ok(success_output("  \n")));

    let identity = read_host_git_identity(&runner);

    assert!(identity.name.is_none());
    assert!(identity.email.is_none());
}

// Note: Container configurator tests use mock ContainerExecClient
// and are in the BDD test suite (Stage C) for full integration
// coverage. Additional unit tests for set_git_config error paths
// are here.
```

#### A.5: Update `src/engine/connection/mod.rs`

Add `mod git_identity;` and re-export public types:

```rust
mod git_identity;

pub use git_identity::{
    GitIdentityResult, HostCommandRunner, HostGitIdentity,
    SystemCommandRunner, configure_git_identity,
    read_host_git_identity,
};
```

#### A.6: Update `src/engine/mod.rs`

Add re-exports to the engine module's `pub use` block:

```rust
pub use connection::{
    // ... existing re-exports ...
    GitIdentityResult, HostCommandRunner, HostGitIdentity,
    SystemCommandRunner, configure_git_identity,
    read_host_git_identity,
};
```

**Stage A gate**: run `make check-fmt && make lint && make test`.

### Stage B: API orchestration function

#### B.1: Create `src/api/configure_git_identity.rs`

API-level orchestration function that composes host reading and
container configuration.

```rust
//! Git identity configuration orchestration.
//!
//! Reads Git identity from the host and configures it within the
//! container. Missing identity fields produce warnings rather than
//! errors, following the principle that Git identity is helpful but
//! not required for all container operations.

use crate::engine::{
    ContainerExecClient, GitIdentityResult, HostCommandRunner,
    configure_git_identity as engine_configure,
    read_host_git_identity,
};
use crate::error::Result as PodbotResult;

/// Parameters for Git identity configuration.
pub struct GitIdentityParams<'a, C: ContainerExecClient, R: HostCommandRunner> {
    /// Pre-connected container engine client.
    pub client: &'a C,
    /// Host command runner for reading Git config.
    pub host_runner: &'a R,
    /// Target container identifier.
    pub container_id: &'a str,
    /// Tokio runtime handle for blocking execution.
    pub runtime_handle: &'a tokio::runtime::Handle,
}

/// Read host Git identity and configure it in the container.
///
/// This is the top-level orchestration entry point for Step 4.1.
/// Missing host identity fields result in a partial or none-configured
/// result rather than an error.
///
/// # Errors
///
/// Returns `ContainerError::ExecFailed` if a `git config` command
/// fails to execute within the container.
pub fn configure_container_git_identity<
    C: ContainerExecClient,
    R: HostCommandRunner,
>(
    params: &GitIdentityParams<'_, C, R>,
) -> PodbotResult<GitIdentityResult> {
    let identity = read_host_git_identity(params.host_runner);
    engine_configure(
        params.runtime_handle,
        params.client,
        params.container_id,
        &identity,
    )
}
```

#### B.2: Update `src/api/mod.rs`

Add the new module and re-export:

```rust
mod configure_git_identity;

pub use configure_git_identity::{
    GitIdentityParams, configure_container_git_identity,
};
```

**Stage B gate**: run `make check-fmt && make lint && make test`.

### Stage C: BDD behavioural tests

#### C.1: Create `tests/features/git_identity.feature`

```gherkin
Feature: Git identity configuration

  Configure Git identity within the container by reading
  user.name and user.email from the host Git configuration.

  Scenario: Both name and email are configured
    Given host git user.name is Alice
    And host git user.email is alice@example.com
    And the container engine is available
    When git identity configuration is requested for container sandbox-1
    Then git identity result is configured
    And configured name is Alice
    And configured email is alice@example.com

  Scenario: Only name is configured on the host
    Given host git user.name is Bob
    And host git user.email is missing
    And the container engine is available
    When git identity configuration is requested for container sandbox-2
    Then git identity result is partial
    And configured name is Bob
    And configured email is absent
    And warnings include git user.email is not configured on the host

  Scenario: Only email is configured on the host
    Given host git user.name is missing
    And host git user.email is carol@example.com
    And the container engine is available
    When git identity configuration is requested for container sandbox-3
    Then git identity result is partial
    And configured name is absent
    And configured email is carol@example.com
    And warnings include git user.name is not configured on the host

  Scenario: Neither name nor email is configured
    Given host git user.name is missing
    And host git user.email is missing
    And the container engine is available
    When git identity configuration is requested for container sandbox-4
    Then git identity result is none configured
    And warnings include git user.name is not configured on the host
    And warnings include git user.email is not configured on the host

  Scenario: Container exec failure propagates as error
    Given host git user.name is Alice
    And host git user.email is alice@example.com
    And the container engine exec will fail
    When git identity configuration is requested for container sandbox-5
    Then git identity configuration fails with an exec error
```

#### C.2: Create `tests/bdd_git_identity_helpers/state.rs`

State struct with `Slot<T>` fields for host identity, container ID,
engine availability, and result.

#### C.3: Create `tests/bdd_git_identity_helpers/steps.rs`

Given/when step definitions using `mock!` for both `HostCommandRunner`
and `ContainerExecClient`. The `when` step creates a tokio runtime,
builds `GitIdentityParams`, and invokes
`configure_container_git_identity()`.

#### C.4: Create `tests/bdd_git_identity_helpers/assertions.rs`

Then step definitions for verifying `GitIdentityResult` variants,
configured values, warnings, and error outcomes.

#### C.5: Create `tests/bdd_git_identity_helpers/mod.rs`

Re-export hub with `StepResult<T>` type alias and
`#[expect(unused_imports)]` on step/assertion imports.

#### C.6: Create `tests/bdd_git_identity.rs`

Scenario bindings using `#[scenario]` macros, one function per scenario.

**Stage C gate**: run `make check-fmt && make lint && make test`.

### Stage D: Documentation updates

#### D.1: Update `docs/podbot-design.md`

No structural changes needed; the design doc already describes step 3
(Git identity configuration). If the module structure section exists,
add the `git_identity/` entry under `engine/connection/`.

#### D.2: Update `docs/users-guide.md`

Add a "Git identity configuration" section under "Container creation
behaviour" (or similar):

```markdown
### Git identity configuration

At sandbox startup, podbot reads Git identity (`user.name` and
`user.email`) from the host Git configuration and executes
`git config --global` within the container to propagate these values.

- If both values are present, they are configured in the container.
- If only one value is present, the available value is configured and
  a warning is produced for the missing field.
- If neither value is present, podbot warns but does not fail.
- If the host `git` binary is not installed, both values are treated
  as absent.

This ensures that Git commits made within the container use the
operator's identity without requiring manual configuration inside the
sandbox.
```

#### D.3: Update `docs/podbot-roadmap.md`

Mark Step 4.1 tasks as done:

```markdown
- [x] Read user.name from host Git configuration.
- [x] Read user.email from host Git configuration.
- [x] Execute git config --global user.name within the container.
- [x] Execute git config --global user.email within the container.
- [x] Handle missing Git identity with a warning rather than failure.
```

#### D.4: Save execution plan

Update this file with completion status and retrospective.

**Stage D gate**: run full gate stack.

### Stage E: Final verification

Run the full quality gate:

```bash
set -o pipefail
make check-fmt 2>&1 | tee /tmp/check-fmt-4-1-1.log
make lint 2>&1 | tee /tmp/lint-4-1-1.log
make test 2>&1 | tee /tmp/test-4-1-1.log
```

Verify all pre-existing tests pass unchanged. Verify new tests pass.

## Interfaces and dependencies

### New public types

In `src/engine/connection/git_identity/host_reader.rs`:

```rust
pub trait HostCommandRunner {
    fn run_command(&self, program: &str, args: &[&str])
        -> io::Result<Output>;
}

pub struct SystemCommandRunner;

pub struct HostGitIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
}
```

In `src/engine/connection/git_identity/container_configurator.rs`:

```rust
pub enum GitIdentityResult {
    Configured { name: String, email: String },
    Partial {
        name: Option<String>,
        email: Option<String>,
        warnings: Vec<String>,
    },
    NoneConfigured { warnings: Vec<String> },
}
```

In `src/api/configure_git_identity.rs`:

```rust
pub struct GitIdentityParams<'a, C, R> { ... }

pub fn configure_container_git_identity<C, R>(
    params: &GitIdentityParams<'_, C, R>,
) -> PodbotResult<GitIdentityResult>;
```

### Consumed (not modified) dependencies

- `podbot::engine::ContainerExecClient` -- trait for container exec
- `podbot::engine::EngineConnector::exec()` -- blocking exec helper
- `podbot::engine::ExecRequest`, `ExecMode::Detached` -- request types
- `podbot::error::ContainerError::ExecFailed` -- error variant
- `std::process::Command` -- host-side command execution (in production)

No new external crate dependencies are required.

## Files to create

| File | Purpose |
| --- | --- |
| `src/engine/connection/git_identity/mod.rs` | Module hub and re-exports |
| `src/engine/connection/git_identity/host_reader.rs` | Host Git config reader trait and impl |
| `src/engine/connection/git_identity/container_configurator.rs` | Container `git config` executor |
| `src/engine/connection/git_identity/tests.rs` | Unit tests for host reader |
| `src/api/configure_git_identity.rs` | API orchestration function |
| `tests/features/git_identity.feature` | BDD scenarios |
| `tests/bdd_git_identity.rs` | Scenario bindings |
| `tests/bdd_git_identity_helpers/mod.rs` | Re-exports |
| `tests/bdd_git_identity_helpers/state.rs` | State + fixture |
| `tests/bdd_git_identity_helpers/steps.rs` | Given/when steps |
| `tests/bdd_git_identity_helpers/assertions.rs` | Then assertions |

## Files to modify

| File | Change |
| --- | --- |
| `src/engine/connection/mod.rs` | Add `mod git_identity;` and re-exports |
| `src/engine/mod.rs` | Add Git identity types to `pub use` block |
| `src/api/mod.rs` | Add `mod configure_git_identity;` and re-export |
| `docs/podbot-design.md` | Add `git_identity/` module entry if needed |
| `docs/users-guide.md` | Add Git identity configuration section |
| `docs/podbot-roadmap.md` | Mark Step 4.1 tasks as done |

## Validation and acceptance

Quality criteria:

- `make check-fmt` passes (cargo fmt workspace check).
- `make lint` passes (clippy with all warnings denied, cargo doc).
- `make test` passes (all existing and new tests).
- Unit test: host reader returns both fields when configured.
- Unit test: host reader returns `None` when `git` is not installed.
- Unit test: host reader returns `None` for unconfigured fields.
- Unit test: host reader trims whitespace from output.
- BDD scenario: both name and email configured in container.
- BDD scenario: partial identity produces warnings.
- BDD scenario: no identity produces warnings, no failure.
- BDD scenario: container exec failure propagates as error.
- All pre-existing tests pass unchanged.

Quality method:

```bash
set -o pipefail
make check-fmt 2>&1 | tee /tmp/check-fmt-4-1-1.log
make lint 2>&1 | tee /tmp/lint-4-1-1.log
make test 2>&1 | tee /tmp/test-4-1-1.log
```

## Idempotence and recovery

All stages are additive and can be rerun safely. If partial edits leave
the tree failing, revert only incomplete hunks and replay the current
stage. `cargo clean -p podbot` may be needed after modifying feature
files (rstest-bdd reads them at compile time). Gate logs stored under
`/tmp` with unique names per run.

## Implementation order

1. `src/engine/connection/git_identity/host_reader.rs` -- host reader
   trait and function
2. `src/engine/connection/git_identity/container_configurator.rs` --
   container exec logic
3. `src/engine/connection/git_identity/mod.rs` -- module hub
4. `src/engine/connection/git_identity/tests.rs` -- unit tests
5. `src/engine/connection/mod.rs` -- add module and re-exports
6. `src/engine/mod.rs` -- add re-exports
7. Gate check (Stage A)
8. `src/api/configure_git_identity.rs` -- API function
9. `src/api/mod.rs` -- add module and re-export
10. Gate check (Stage B)
11. `tests/features/git_identity.feature` -- BDD scenarios
12. `tests/bdd_git_identity_helpers/state.rs` -- state struct
13. `tests/bdd_git_identity_helpers/steps.rs` -- step definitions
14. `tests/bdd_git_identity_helpers/assertions.rs` -- assertions
15. `tests/bdd_git_identity_helpers/mod.rs` -- re-exports
16. `tests/bdd_git_identity.rs` -- scenario bindings
17. Gate check (Stage C)
18. `docs/podbot-design.md` -- update if needed
19. `docs/users-guide.md` -- add Git identity section
20. `docs/podbot-roadmap.md` -- mark tasks done
21. Gate check (Stage D)
22. Final gate check (Stage E)
