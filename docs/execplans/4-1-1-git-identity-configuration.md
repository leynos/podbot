# Step 4.1.1: Git identity configuration

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: IN_PROGRESS

No `PLANS.md` file exists in this repository as of 2026-04-07, so this ExecPlan
is the governing implementation document for this task.

## Purpose and big picture

Implement Step 4.1 (Git identity configuration) from
`docs/podbot-roadmap.md`. The goal is to read `user.name` and `user.email`
from the host Git configuration and apply them inside the container via
`git config --global`, so that Git commits made by agents carry the correct
identity. When the host has no Git identity configured, podbot must emit a
warning rather than failing.

After this change, callers can configure Git identity within a container using
host settings. The behaviour is observable through unit tests and behaviour
tests that validate both happy paths (identity present) and unhappy paths
(identity missing or partially missing) without requiring a live daemon.

This plan also covers required documentation updates:

- Record the Git identity configuration design in `docs/podbot-design.md`.
- Update user-facing behaviour in `docs/users-guide.md`.
- Mark the relevant roadmap entry as done once acceptance criteria are met.

## Constraints

- Keep existing engine, configuration, and API behaviour unchanged.
- Do not add new third-party dependencies unless a blocker is documented in the
  `Decision log` and approved.
- Keep module-level `//!` documentation in every Rust module touched.
- Avoid `unwrap` and `expect` outside test code.
- Use `rstest` fixtures for unit tests and `rstest-bdd` v0.5.0 for behavioural
  tests.
- Keep files under 400 lines where practical; split modules when a file would
  exceed this limit.
- Preserve current public configuration semantics unless a roadmap requirement
  explicitly requires a change.
- Use en-GB-oxendict spelling in documentation and comments.
- Prefer Makefile targets for verification (`make check-fmt`, `make lint`, and
  `make test`).
- Git identity reading and application must use dependency injection (trait
  seams) so that unit tests run deterministically without a live Git
  installation or container daemon.

## Tolerances (exception triggers)

- Scope: if implementation requires edits in more than 15 files or more than
  600 net lines, stop and confirm scope.
- Public API: if an existing public API must break compatibility, stop and
  confirm the migration strategy.
- Dependencies: if a new dependency is required for Git configuration reading
  or command execution mocking, stop and confirm before adding it.
- Iterations: if `make lint` or `make test` still fails after two fix passes,
  stop and document the blocker with log evidence.

## Risks

- Risk: Reading host Git configuration may behave differently across platforms
  (Linux, macOS, Windows) or when Git is not installed. Severity: medium
  Likelihood: medium Mitigation: use `std::process::Command` to invoke
  `git config --get` and handle non-zero exit codes as "not configured" rather
  than fatal errors. Wrap this in a trait seam for deterministic testing.

- Risk: Container exec for `git config --global` may fail if Git is not
  installed in the container image. Severity: low Likelihood: low Mitigation:
  map exec failures to warnings rather than hard errors, consistent with the
  roadmap requirement that missing identity produces a warning but does not
  block execution.

- Risk: Behaviour tests may become flaky if they depend on the host Git state.
  Severity: medium Likelihood: medium Mitigation: use dependency injection with
  deterministic test doubles for the Git configuration reader and container
  exec client. No test should depend on host Git configuration.

## Progress

- [x] (2026-04-07 UTC) Reviewed roadmap, design, testing guides, and current
      engine code structure.
- [x] (2026-04-07 UTC) Drafted this ExecPlan at
      `docs/execplans/4-1-1-git-identity-configuration.md`.
- [ ] Implement Git identity reader with trait seam.
- [ ] Implement container Git configuration applicator.
- [ ] Add `rstest` unit tests for happy, unhappy, and edge paths.
- [ ] Add `rstest-bdd` v0.5.0 scenarios for Git identity behaviour.
- [ ] Update design, user guide, and roadmap documentation.
- [ ] Run quality gates and capture logs.

## Surprises and discoveries

(To be populated as work proceeds.)

## Decision log

- Decision: introduce a `GitIdentityReader` trait and a `GitIdentity` data
  type to abstract host Git configuration reading. Rationale: this follows the
  dependency injection pattern established in
  `docs/reliable-testing-in-rust-via-dependency-injection.md` and allows
  deterministic testing without mutating host Git configuration. The trait will
  have a `read_git_identity(&self) -> GitIdentity` method where `GitIdentity`
  contains `Option<String>` fields for `name` and `email`.
  Date/Author: 2026-04-07 / DevBoxer.

- Decision: place the Git identity module at
  `src/engine/connection/git_identity/` rather than in a new top-level module.
  Rationale: Git identity configuration is part of the container orchestration
  lifecycle (Step 3 in the design document's execution flow) and belongs with
  other container-lifecycle operations under `engine/connection`. The module
  will contain the reader trait, the container applicator, and associated types.
  Date/Author: 2026-04-07 / DevBoxer.

- Decision: use `std::process::Command` for the production `GitIdentityReader`
  implementation rather than parsing `.gitconfig` files directly. Rationale:
  `git config --get` respects the full Git configuration precedence chain
  (system, global, local, worktree, command-line) and handles includes,
  conditional includes, and platform-specific paths. Direct file parsing would
  need to replicate all of this. The command approach is simpler, more correct,
  and aligns with the existing pattern of using command execution for host
  interactions. Date/Author: 2026-04-07 / DevBoxer.

- Decision: treat missing Git identity as a partial result rather than an
  error. Rationale: the roadmap explicitly requires "Handle missing Git
  identity with a warning rather than failure." The `GitIdentity` struct will
  use `Option<String>` for both `name` and `email`, and the container
  application function will skip missing fields with a log warning rather than
  returning an error. Date/Author: 2026-04-07 / DevBoxer.

- Decision: reuse the existing `ContainerExecClient` trait for executing
  `git config --global` within the container rather than introducing a new
  abstraction. Rationale: the exec infrastructure already provides deterministic
  test seams, error mapping, and async/sync bridging. Adding another layer would
  increase complexity without benefit.
  Date/Author: 2026-04-07 / DevBoxer.

## Outcomes and retrospective

(To be populated on completion.)

## Context and orientation

The Git identity configuration step is Step 3 in the design document's
execution flow (see `docs/podbot-design.md`, "Execution flow"):

> 3. **Configure Git identity** by reading `user.name` and `user.email` from
>    the host and executing `git config --global` within the container.

Current engine code is organised under `src/engine/connection/` and supports:

- socket resolution from config, environment, and defaults;
- connection establishment via Bollard;
- health-check verification with timeout handling;
- container creation with configurable security options;
- credential injection via tar archive upload;
- interactive and protocol-safe command execution.

Relevant files for this work:

- `src/engine/mod.rs` — engine module re-exports
- `src/engine/connection/mod.rs` — connection module and `EngineConnector`
- `src/engine/connection/exec/mod.rs` — `ContainerExecClient` trait and exec
  types
- `src/error.rs` — semantic error types (`ContainerError::ExecFailed`)
- `src/api/mod.rs` — orchestration API stubs
- `docs/podbot-design.md` — execution flow description
- `docs/users-guide.md` — user documentation
- `docs/podbot-roadmap.md` — roadmap with Step 4.1

Key requirement translation for Step 4.1.1:

- Read `user.name` from host Git configuration.
- Read `user.email` from host Git configuration.
- Execute `git config --global user.name` within the container.
- Execute `git config --global user.email` within the container.
- Handle missing Git identity with a warning rather than failure.

## Plan of work

### Stage A: Introduce Git identity module and types

Create a dedicated submodule at `src/engine/connection/git_identity/` with:

- `GitIdentity` struct: holds `Option<String>` for `name` and `email`, with
  predicate methods (`has_name`, `has_email`, `is_empty`, `is_complete`).
- `GitIdentityReader` trait: `fn read_git_identity(&self) -> GitIdentity`.
- `SystemGitIdentityReader`: production implementation using
  `std::process::Command` to run `git config --get user.name` and
  `git config --get user.email`.
- `configure_git_identity_async`: async function that reads host identity and
  applies it to the container via `ContainerExecClient` exec calls, logging
  warnings for missing fields.

Target outcome: a testable Git identity module that compiles and is wired into
the engine re-exports.

Validation gate:

- new module compiles;
- no behavioural changes to existing tests.

### Stage B: Implement host Git identity reading

Implement `SystemGitIdentityReader`:

- Run `git config --get user.name` via `std::process::Command`.
- Run `git config --get user.email` via `std::process::Command`.
- Non-zero exit codes (key not set) → `None` for that field.
- Command execution failures (Git not installed) → `None` for both fields.
- Trim whitespace from output.

Target outcome: production reader that gracefully handles all host states.

Validation gate:

- unit tests validate reader behaviour with mock command executor.

### Stage C: Implement container Git identity application

Implement `configure_git_identity_async`:

- Accept `ContainerExecClient`, container ID, and `GitIdentity`.
- For each present field, execute `git config --global user.name <value>` or
  `git config --global user.email <value>` in the container.
- Log a warning (to stderr via `eprintln!` or a structured log) for each
  missing field.
- If both fields are missing, log a single consolidated warning.
- Container exec failures for individual config commands are logged as warnings
  and do not abort the overall operation.
- Return a `GitIdentityResult` indicating which fields were applied.

Target outcome: deterministic, testable application of Git identity to a
container.

Validation gate:

- unit tests validate generated exec commands and warning behaviour.

### Stage D: Add unit tests with rstest

Implement module-local unit tests using `rstest` fixtures and parameterised
cases. Coverage must include:

- happy path: both `user.name` and `user.email` present on host;
- happy path: identity applied to container via two exec calls;
- edge path: only `user.name` present (email missing);
- edge path: only `user.email` present (name missing);
- unhappy path: neither field present → warnings emitted, no exec calls;
- unhappy path: host Git not installed → graceful degradation;
- unhappy path: container exec fails for one field → other field still applied.

Use mock implementations of `GitIdentityReader` and `ContainerExecClient`
for determinism.

Validation gate:

- `cargo test` for the targeted module passes before proceeding.

### Stage E: Add behavioural tests with rstest-bdd v0.5.0

Add a dedicated feature file at `tests/features/git_identity.feature` and
scenario bindings covering observable behaviour in Given-When-Then form.

Proposed scenarios:

- Git identity configured on host → both fields applied to container;
- Only user name configured → name applied, warning for missing email;
- Only user email configured → email applied, warning for missing name;
- No Git identity configured → warning emitted, no failure;
- Container exec fails → graceful degradation with warning.

Use state fixtures and step helpers in a new helper module mirroring the
existing engine BDD structure:

- `tests/bdd_git_identity.rs`
- `tests/bdd_git_identity_helpers/mod.rs`
- `tests/bdd_git_identity_helpers/state.rs`
- `tests/bdd_git_identity_helpers/steps.rs`
- `tests/bdd_git_identity_helpers/assertions.rs`

Validation gate:

- new behavioural tests pass under `make test` with `rstest-bdd` macros.

### Stage F: Documentation and roadmap updates

Update `docs/podbot-design.md` with a concise subsection documenting:

- Git identity reading strategy (command-based, not file parsing);
- container application approach (exec-based);
- warning-not-failure behaviour for missing identity.

Update `docs/users-guide.md` with user-visible behaviour for Git identity
configuration:

- when identity is applied;
- what happens when identity is missing;
- relationship to host `git config` settings.

Update `docs/podbot-roadmap.md` by marking Step 4.1 entries as done once tests
and quality gates pass.

Validation gate:

- documentation is accurate to implemented behaviour;
- roadmap checkbox state matches delivered scope.

### Stage G: Quality gates and commit

Run required quality gates and retain logs, then commit atomically with a
message that explains what changed and why.

Validation gate:

- `make check-fmt`, `make lint`, and `make test` all exit zero;
- commit includes only intended files.

## Concrete steps

All commands run from repository root: `/home/user/project`.

1. Create `src/engine/connection/git_identity/mod.rs` with types and traits.
2. Implement `SystemGitIdentityReader` using `std::process::Command`.
3. Implement `configure_git_identity_async` using `ContainerExecClient`.
4. Wire module into `src/engine/connection/mod.rs` and `src/engine/mod.rs`.
5. Add `rstest` unit tests for reader, applicator, and integration paths.
6. Create `tests/features/git_identity.feature` with BDD scenarios.
7. Create `tests/bdd_git_identity.rs` and helper modules.
8. Update `docs/podbot-design.md` with Git identity design.
9. Update `docs/users-guide.md` with user-facing behaviour.
10. Mark roadmap entry complete.
11. Run verification with log capture:

    ```sh
    set -o pipefail
    make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-$(git branch --show).out

    set -o pipefail
    make lint 2>&1 | tee /tmp/lint-podbot-$(git branch --show).out

    set -o pipefail
    make test 2>&1 | tee /tmp/test-podbot-$(git branch --show).out
    ```

12. Review diffs and commit.

## Validation and acceptance

Implementation is complete when all conditions below are true:

- A callable `configure_git_identity_async` path exists and applies host Git
  identity to a container.
- Host Git identity reading is abstracted behind a `GitIdentityReader` trait
  with a production implementation and a mock for testing.
- Both `user.name` and `user.email` are read from the host and applied to
  the container.
- Missing Git identity fields produce warnings rather than errors.
- Container exec failures for identity commands produce warnings rather than
  aborting the operation.
- Unit tests cover happy, unhappy, and edge paths with deterministic mocks.
- Behavioural tests implemented with `rstest-bdd` v0.5.0 pass.
- `docs/podbot-design.md` records the Git identity configuration approach.
- `docs/users-guide.md` describes user-visible behaviour changes.
- `docs/podbot-roadmap.md` marks Step 4.1 entries as done.
- `make check-fmt`, `make lint`, and `make test` all pass.

## Idempotence and recovery

- Code-edit stages are safe to rerun; repeated edits should converge on the
  same end state.
- If test failures occur, capture output logs, fix one class of failure at a
  time, and rerun the specific failing command.
- If a tolerance trigger is hit, stop implementation, update `Decision log`,
  and wait for direction before expanding scope.

## Artefacts and notes

Expected verification artefacts:

- `/tmp/check-fmt-podbot-<branch>.out`
- `/tmp/lint-podbot-<branch>.out`
- `/tmp/test-podbot-<branch>.out`

Keep these paths in the final implementation summary for traceability.

## Interfaces and dependencies

Planned interfaces (names may be refined during implementation, but intent is
fixed):

- `GitIdentity`: data struct holding `Option<String>` for `name` and `email`.
- `GitIdentityReader` trait: abstracts host Git configuration reading.
- `SystemGitIdentityReader`: production implementation using
  `std::process::Command`.
- `configure_git_identity_async<C: ContainerExecClient>`: applies Git identity
  to a container.
- `GitIdentityResult`: outcome struct reporting which fields were applied and
  which were skipped.

Dependencies remain unchanged. The implementation uses only `std::process` for
host Git reading and the existing `ContainerExecClient` trait for container
exec.

## Revision note

Initial draft created on 2026-04-07 to plan Step 4.1.1 Git identity
configuration work, including implementation, testing, documentation, and
roadmap-completion tasks.
