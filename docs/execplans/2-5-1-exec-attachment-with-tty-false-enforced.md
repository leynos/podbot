# Step 2.5.1: Exec attachment with `tty = false` enforced

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose / big picture

Podbot's exec subsystem currently supports two execution modes: Attached
(streams connected, optional TTY) and Detached (no streams at all). Step 2.5 of
the roadmap requires protocol-safe execution where an exec session has streams
connected for proxying protocol bytes between an IDE client and a containerised
app server, but TTY is permanently disabled. TTY escape sequences and terminal
framing would corrupt protocol byte streams, so the enforcement must be
structural, not caller-discipline.

After this change, a library consumer can write:

```rust
let request = ExecRequest::new("container", cmd, ExecMode::Protocol)?;
assert!(!request.tty());  // always false, even after .with_tty(true)
```

The new `ExecMode::Protocol` variant connects streams (stdin, stdout, stderr
forwarded) but permanently enforces `tty = false`. No SIGWINCH listener is
registered. No `resize_exec` calls are made. Existing Attached and Detached
behaviour is unchanged.

Observable success: `make check-fmt`, `make lint`, and `make test` all pass.
New unit tests prove TTY enforcement. New BDD scenarios validate protocol-mode
execution end-to-end. The first checkbox under Step 2.5 in the roadmap is
marked done.

## Constraints

- No single code file may exceed 400 lines.
  `src/engine/connection/exec/tests.rs` is currently at 386 lines, so new
  protocol-mode tests must go in a new submodule file.
- Every Rust module must begin with a `//!` module-level doc comment.
- en-GB-oxendict spelling ("-ize" / "-yse" / "-our") in all comments and
  documentation.
- No `unwrap()` or `expect()` in production code (clippy denies `unwrap_used`
  and `expect_used`).
- Use directory modules (`mod.rs`), not self-named files
  (`clippy::self_named_module_files` is denied).
- Use `rstest` for unit tests, `rstest-bdd` v0.5.0 for behavioural tests.
- BDD step functions must use `StepResult<T> = Result<T, String>` (no
  `expect`/`panic`).
- Existing public API signatures for `ExecMode::Attached` and
  `ExecMode::Detached` must continue to work identically.
- No new external crate dependencies.
- `make check-fmt`, `make lint`, and `make test` must pass before any commit.
- Commit messages use imperative mood; atomic commits; one logical unit per
  commit.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 20 files or 500 net
  lines of code, stop and escalate.
- Interface: adding `ExecMode::Protocol` is a public API addition, which is
  acceptable. If any existing public API signature must change beyond adding a
  new enum variant, stop and escalate.
- Dependencies: if a new external dependency is required, stop and escalate.
- Iterations: if tests still fail after three focused fix iterations on any
  single gate, stop and escalate.
- File budget: if any file exceeds 400 lines after edits, extract into a
  submodule before proceeding.

## Risks

- Risk: Adding a third `ExecMode` variant breaks exhaustive `match` blocks
  throughout the codebase and tests. Severity: medium. Likelihood: high.
  Mitigation: the compiler flags every non-exhaustive match. The number of
  match sites is bounded (approximately five in production code, eight in
  tests). Each is straightforward to extend.

- Risk: `tests.rs` is at 386/400 lines and cannot absorb new test cases
  inline. Severity: low. Likelihood: certain. Mitigation: extract protocol-mode
  tests into a new `src/engine/connection/exec/tests/protocol_helpers.rs`
  submodule, following the existing pattern of `detached_helpers.rs`.

- Risk: BDD step helpers contain match arms on `ExecMode::Attached` and
  `ExecMode::Detached` that must be extended for Protocol. Severity: low.
  Likelihood: high. Mitigation: Protocol mode's start-exec configuration is
  identical to Attached except `tty = false`; the new match arm reuses the
  attached stream setup.

## Progress

- [x] (2026-03-18) Stage A: Add `ExecMode::Protocol` variant and update
      `ExecRequest` logic.
- [x] (2026-03-18) Stage B: Update exec dispatch match arms in
      `exec_async_with_terminal_size_provider`.
- [x] (2026-03-18) Stage C: Update `src/api/exec.rs` doc comment.
- [x] (2026-03-18) Stage D: Unit tests for protocol mode in new
      `protocol_helpers.rs`.
- [x] (2026-03-18) Stage E: BDD feature scenarios and step helpers for
      protocol mode.
- [x] (2026-03-18) Stage F: Documentation updates (design doc, users guide,
      roadmap).
- [x] (2026-03-18) Stage G: Verification gates passed (`make check-fmt`,
      `make lint`, `make test`, `make markdownlint` all exit 0).

## Surprises & discoveries

- Observation: `clippy::panic_in_result_fn` fires on `#[rstest]` test
  functions that return `TestResult` and contain `assert!` macros directly in
  the function body. Evidence: lint error during `make lint` on
  protocol_helpers.rs tests. Impact: assertions must be extracted into
  non-Result-returning helper functions, matching the existing codebase pattern
  in `detached_helpers.rs` and the top-level `tests.rs`.

## Decision log

- Decision: Use a new `ExecMode::Protocol` variant rather than relying on
  caller discipline with `.with_tty(false)`. Rationale: `ExecRequest::new()`
  currently defaults `tty` to `true` for `ExecMode::Attached`. A caller could
  construct an attached request and forget to call `.with_tty(false)`,
  resulting in TTY framing that corrupts protocol streams. A dedicated variant
  makes it impossible to construct a protocol-mode request with `tty = true`,
  because enforcement is in the constructor and builder, not in caller
  discipline. Option B (caller discipline) was rejected because it offers no
  compile-time safety.

- Decision: `ExecMode::is_attached()` returns true for Protocol.
  Rationale: Protocol mode needs stream attachment (stdin/stdout/stderr
  forwarded). Only TTY allocation differs from interactive Attached mode. The
  Bollard options builders already delegate to `is_attached()` for stream flags
  and `tty()` for terminal allocation, so extending `is_attached()` to include
  Protocol gives correct behaviour automatically.

- Decision: No changes to `build_create_exec_options`,
  `build_start_exec_options`, `attached.rs`, or `terminal.rs`. Rationale: These
  functions already delegate to `request.mode().is_attached()` for stream
  attachment and `request.tty()` for TTY/resize decisions. Since Protocol sets
  `is_attached() == true` and `tty() == false`, existing logic produces correct
  Bollard options and skips resize handling without modification.

## Outcomes & retrospective

Shipped:

- New `ExecMode::Protocol` variant providing protocol-safe execution with
  permanent `tty = false` enforcement.
- Seven new unit tests in `protocol_helpers.rs` covering constructor
  enforcement, tty override rejection, Bollard options correctness, end-to-end
  success/failure paths.
- Two new BDD scenarios validating protocol execution with exit codes 0 and 1.
- Updated design doc, users guide, and roadmap.
- All four gates pass: `make check-fmt`, `make lint`, `make test`,
  `make markdownlint`.

Risk outcomes:

- Non-exhaustive match arms were resolved mechanically in five locations
  (production code and test helpers). The compiler flagged every instance.
- The 386-line `tests.rs` file limit was respected by placing all new tests
  in `tests/protocol_helpers.rs` (one new `mod` declaration added).

Follow-up beyond Step 2.5.1:

- None required. The Protocol mode variant is ready for use by the hosting
  subsystem (Step 2.5 remaining tasks).

## Context and orientation

The podbot project is a Rust application that manages sandboxed AI agent
containers via Bollard (a Docker/Podman API client). The exec subsystem handles
running commands inside containers.

Key types and files:

- `src/engine/connection/exec/mod.rs` (347 lines): defines `ExecMode` (enum
  with `Attached` and `Detached`), `ExecRequest` (struct holding container_id,
  command, env, mode, and tty), `ExecResult`, the `ContainerExecClient` trait
  (abstracting Bollard calls), and the `EngineConnector::exec_async()` method
  that orchestrates the create/start/inspect lifecycle. Also contains
  `build_create_exec_options()` and `build_start_exec_options()` which map an
  `ExecRequest` to Bollard API option structs.

- `src/engine/connection/exec/attached.rs` (317 lines): contains
  `run_attached_session_async()` which wires stdin/stdout/stderr between the
  local process and the container exec session. Spawns a stdin forwarding task,
  manages SIGWINCH resize handling, and runs an output loop forwarding
  `LogOutput` chunks.

- `src/engine/connection/exec/terminal.rs` (130 lines):
  `TerminalSizeProvider` trait, `SystemTerminalSizeProvider`,
  `resize_exec_to_current_terminal_async()`, and SIGWINCH listener setup. The
  function `maybe_sigwinch_listener` already checks `request.tty()` and returns
  `None` when false, so Protocol mode will naturally skip SIGWINCH registration.

- `src/api/exec.rs` (71 lines): `ExecParams<C>` struct and `exec()`
  function, the library-facing orchestration entry point.

- `src/engine/connection/exec/tests.rs` (386 lines): unit tests with
  `MockExecClient`. Near the 400-line file limit.

- `src/engine/connection/exec/tests/detached_helpers.rs` (117 lines),
  `lifecycle_helpers.rs` (39 lines), `validation_tests.rs` (60 lines): test
  helper submodules.

- `tests/features/interactive_exec.feature` (45 lines): BDD feature with
  five existing scenarios.

- `tests/bdd_interactive_exec.rs` (47 lines): BDD test harness.

- `tests/bdd_interactive_exec_helpers/` (`mod.rs`, `state.rs`, `steps.rs`,
  `assertions.rs`): BDD helper modules.

How `ExecMode` flows through the system today:

1. `ExecMode::Attached` or `ExecMode::Detached` is chosen by the caller.
2. `ExecRequest::new()` sets `tty = mode.is_attached()` (true for Attached,
   false for Detached).
3. `ExecRequest::with_tty()` allows override but clamps to
   `self.mode.is_attached() && tty`.
4. `build_create_exec_options()` uses `request.mode().is_attached()` to set
   `attach_stdin/stdout/stderr`, and `attached && request.tty()` for `tty`.
5. `build_start_exec_options()` uses `request.mode().is_attached()` for
   `detach`, and `request.mode().is_attached() && request.tty()` for `tty`.
6. `exec_async_with_terminal_size_provider()` matches on
   `(request.mode(), start_result)` with four arms.
7. In the Attached path, `run_attached_session_async` checks `request.tty()`
   to decide whether to register SIGWINCH and call resize.

What Protocol mode needs to do differently from Attached: `is_attached()`
returns true (streams connected). `tty` is always false (enforced in
constructor, cannot be overridden). No SIGWINCH listener, no resize calls
(already handled by `request.tty()` being false). The dispatch match treats
Protocol the same as Attached for stream handling.

## Plan of work

### Stage A: Add `ExecMode::Protocol` variant and update `ExecRequest`

In `src/engine/connection/exec/mod.rs`, add a `Protocol` variant to the
`ExecMode` enum. Update `is_attached()` to return true for both `Attached` and
`Protocol`. Add an `is_protocol()` query method. Change the tty default in
`ExecRequest::new()` from `mode.is_attached()` to `mode == ExecMode::Attached`,
so Protocol starts with `tty = false`. Change `with_tty()` from
`self.mode.is_attached() && tty` to
`matches!(self.mode, ExecMode::Attached) && tty`, so Protocol cannot have tty
overridden to true.

### Stage B: Update exec dispatch match

In the same file, update the match in `exec_async_with_terminal_size_provider`
to combine the Attached and Protocol arms using `|` patterns:

```rust
(ExecMode::Attached | ExecMode::Protocol,
 StartExecResults::Attached { output, input }) => { ... }
(ExecMode::Attached | ExecMode::Protocol,
 StartExecResults::Detached) => { return Err(...) }
```

No changes needed to `build_create_exec_options`, `build_start_exec_options`,
`attached.rs`, or `terminal.rs` since they delegate to `is_attached()` and
`tty()`.

### Stage C: Update `src/api/exec.rs`

Update the doc comment on the `tty` field of `ExecParams` to note it is ignored
for Protocol and Detached modes.

### Stage D: Unit tests for protocol mode

Create `src/engine/connection/exec/tests/protocol_helpers.rs` with helper
functions and `#[rstest]` tests covering:

1. Protocol mode enforces `tty=false` in constructor.
2. Protocol mode rejects `tty` override via `.with_tty(true)`.
3. Protocol exec succeeds end-to-end (mock create/start/inspect) with correct
   exit code and `resize_exec` never called.
4. Protocol mode rejects detached daemon response.

Add `mod protocol_helpers;` to `tests.rs`.

### Stage E: BDD scenarios for protocol mode

Add two new scenarios to `tests/features/interactive_exec.feature`:

1. Protocol execution succeeds with tty disabled (exit code 0).
2. Protocol execution returns non-zero exit code.

Add `#[scenario]` wiring in `tests/bdd_interactive_exec.rs`. Add
`#[given("protocol execution mode is selected")]` step in
`tests/bdd_interactive_exec_helpers/steps.rs`. Extend
`configure_start_exec_expectation` and `configure_resize_expectation` match
arms for `ExecMode::Protocol`.

### Stage F: Documentation updates

Update `docs/podbot-design.md` (Interactive exec semantics section) to describe
Protocol mode. Update `docs/users-guide.md` (exec section) with a note about
`ExecMode::Protocol`. Check the first task under Step 2.5 in
`docs/podbot-roadmap.md`.

### Stage G: Verification gates

Run `make check-fmt`, `make lint`, `make test`, and
`MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint`. Fix and rerun if
any fail.

## Concrete steps

All commands are run from `/home/user/project`.

Stage A verification:

```bash
cargo check --all-targets --all-features 2>&1 | head -20
```

Expected: compiler errors from non-exhaustive matches in test code (resolved in
Stage D/E).

Stage D/E/F verification:

```bash
set -o pipefail
cargo clean -p podbot
make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-2-5-1.out
make lint 2>&1 | tee /tmp/lint-podbot-2-5-1.out
make test 2>&1 | tee /tmp/test-podbot-2-5-1.out
```

Expected: all three exit 0.

## Validation and acceptance

Quality criteria:

- `make check-fmt` exits 0.
- `make lint` exits 0.
- `make test` exits 0, including new unit tests in `protocol_helpers.rs` and
  new BDD scenarios.
- All existing tests pass unchanged.

Quality method:

- `ExecRequest::new("c", cmd, ExecMode::Protocol)` yields `tty() == false`.
- `.with_tty(true)` on a Protocol request still yields `tty() == false`.
- Protocol exec connects streams (`attach_*: true`) but sets `tty: false`.
- `resize_exec` is never called during Protocol execution.
- Existing Attached and Detached behaviour is unchanged.

## Idempotence and recovery

Each stage is additive and can be rerun safely. If a partial edit leaves the
tree failing, revert only the incomplete hunks and replay from the last passing
stage. After modifying `.feature` files, run `cargo clean -p podbot` to ensure
rstest-bdd picks up the changes (feature files are read at compile time and
incremental compilation does not track them).

## Artifacts and notes

(To be populated during implementation.)

## Interfaces and dependencies

The end state requires the following interface in
`src/engine/connection/exec/mod.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecMode {
    Attached,
    Detached,
    /// Attach streams for protocol proxying with tty permanently disabled.
    Protocol,
}

impl ExecMode {
    #[must_use]
    const fn is_attached(self) -> bool {
        matches!(self, Self::Attached | Self::Protocol)
    }

    #[must_use]
    const fn is_protocol(self) -> bool {
        matches!(self, Self::Protocol)
    }
}
```

The `ExecRequest::new()` constructor enforces
`tty = (mode == ExecMode::Attached)`. The `with_tty()` builder enforces
`tty = matches!(self.mode, ExecMode::Attached) && tty`.

No new traits, no new crate dependencies. The `ContainerExecClient` trait is
unchanged. Bollard options builders are unchanged.
