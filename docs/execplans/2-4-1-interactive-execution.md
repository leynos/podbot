# Step 2.4.1: Interactive execution via Bollard

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE (2026-02-22)

No `PLANS.md` file exists in this repository as of 2026-02-22, so this ExecPlan
is the governing implementation document for Step 2.4 interactive execution.

## Purpose and big picture

Complete roadmap Step 2.4 in `docs/podbot-roadmap.md` by implementing
interactive command execution in running containers with proper terminal
attachment semantics.

After this change, `podbot exec` will support:

- attached interactive execution with TTY wiring and stream forwarding;
- detached execution mode;
- terminal resize propagation on `SIGWINCH` for interactive sessions;
- accurate command exit-code capture and propagation.

The feature is complete only when unit tests (`rstest`) and behavioural tests
(`rstest-bdd` v0.5.0) cover happy, unhappy, and edge cases, required docs are
updated (`docs/podbot-design.md`, `docs/users-guide.md`), roadmap Step 2.4
items are checked done, and `make check-fmt`, `make lint`, and `make test` all
pass.

## Constraints

- Keep scope to roadmap Step 2.4 only; do not implement unrelated Phase 3+
  behaviour.
- Preserve existing socket resolution precedence and engine connection entry
  points in `EngineConnector`.
- Keep error semantics typed via `PodbotError` and `ContainerError`.
- Avoid `unwrap`/`expect` in production code.
- Use `rstest` for unit tests and `rstest-bdd` v0.5.0 for behavioural tests.
- Keep Rust modules under the 400-line guidance by splitting helpers where
  needed.
- Preserve module-level `//!` documentation in all touched Rust modules.
- Use en-GB-oxendict spelling in documentation changes.
- Use Make targets for verification gates: `make check-fmt`, `make lint`,
  `make test`.
- Commit only logically atomic, fully gated changes.

## Tolerances (exception triggers)

- Scope tolerance: stop and escalate if implementation needs more than 16 files
  or 650 net lines beyond this plan.
- Interface tolerance: stop and escalate if `podbot exec` CLI syntax must break
  existing invocation form `podbot exec <container> -- <command...>`.
- Dependency tolerance: stop and escalate before adding any new crate.
- Runtime tolerance: stop and escalate if cross-platform signal handling cannot
  be implemented without non-trivial platform-specific divergence.
- Test tolerance: stop and escalate if required gates still fail after two
  focused fix iterations.
- Behaviour tolerance: stop and escalate if detached-mode exit-code semantics
  cannot be expressed without introducing a new command or persistent state
  model.

## Risks

- Risk: terminal sizing for `SIGWINCH` may require low-level platform handling.
  Severity: medium. Likelihood: medium. Mitigation: isolate size capture behind
  a small helper seam and test with injected values.
- Risk: attached stream forwarding can create deadlocks if stdin/stdout tasks do
  not close correctly. Severity: high. Likelihood: medium. Mitigation: model
  input/output (IO) forwarding as explicit tasks with clear shutdown ordering
  and unit tests for close/error paths.
- Risk: detached mode semantics are underspecified by roadmap wording. Severity:
  medium. Likelihood: medium. Mitigation: define and document one explicit
  behaviour (no stream attachment, but wait for and return exit code).
- Risk: current `main.rs` command handlers are mostly stubs, so wiring exec may
  reveal orchestration seams missing for later steps. Severity: medium.
  Likelihood: high. Mitigation: keep Step 2.4 narrowly focused on `exec` path,
  reusing existing `EngineConnector` connect-with-fallback behaviour.

## Context and orientation

Current implementation and docs anchors:

- Roadmap Step 2.4 tasks and completion criteria:
  `docs/podbot-roadmap.md`.
- Design flow already calls for starting agent attached to terminal and cites
  Bollard exec-with-TTY support: `docs/podbot-design.md`.
- CLI `exec` surface exists but is stubbed:
  `src/config/cli.rs`, `src/main.rs`.
- Engine wrapper currently covers connect, health check, create, and upload but
  has no exec module yet: `src/engine/connection/mod.rs`,
  `src/engine/connection/create_container/mod.rs`,
  `src/engine/connection/upload_credentials/mod.rs`.
- Existing behavioural test style for engine/container features uses
  `rstest-bdd` helpers, feature files, scenario state slots, and mock-backed
  step modules: `tests/bdd_engine_connection.rs`,
  `tests/bdd_container_creation.rs`, `tests/bdd_credential_injection.rs` plus
  helper directories.

## Agent team execution model

Use a four-lane team for implementation so each lane remains small and
reviewable.

Lane A (engine exec core owner):

- Own new engine exec abstractions and request/response types.
- Implement attached and detached exec flows, resize propagation hook points,
  and exit-code capture.

Lane B (CLI and orchestration owner):

- Own `ExecArgs` shape updates and `main.rs` dispatch wiring.
- Own CLI-visible exit-code behaviour and user-facing messages.

Lane C (test owner):

- Own `rstest` unit coverage for exec module.
- Own `rstest-bdd` feature/scenario updates for behavioural validation.

Lane D (docs and roadmap owner):

- Own `docs/podbot-design.md` design-decision updates.
- Own `docs/users-guide.md` user-visible behaviour updates.
- Mark `docs/podbot-roadmap.md` Step 2.4 items done only after all gates pass.

Coordination rule:

- Merge Lane A first, then Lane B, then Lane C, then Lane D, rebasing each lane
  onto the previous lane tip before final gate replay.

## Plan of work

### Stage A: Add exec module and typed request model

Create an exec-focused engine module at `src/engine/connection/exec/mod.rs` and
wire it from `src/engine/connection/mod.rs` and `src/engine/mod.rs`.

Introduce additive types similar to existing create/upload seams:

- `ExecMode` (attached or detached).
- `ExecRequest` containing container id, command argv, optional env, mode,
  and tty intent.
- `ExecResult` containing exec id and exit code.
- trait abstraction(s) over the Bollard calls needed for create/start/inspect/
  resize so unit tests can run without a daemon.

Mapping rules:

- Validate command vector is non-empty before engine calls.
- Build Bollard exec options from `ExecRequest` deterministically.
- Map engine failures to `ContainerError::ExecFailed` including container id.

### Stage B: Implement attached and detached execution flows

Implement async-first execution APIs under `EngineConnector` with a sync
wrapper, matching existing module patterns.

Attached flow must:

- create exec with `tty=true` when interactive mode is selected;
- start exec with IO streams attached;
- forward stdin to exec stdin and exec stdout/stderr to local streams;
- await exec completion and inspect exit code.

Detached flow must:

- create/start exec without stream attachment;
- wait for completion via inspect loop;
- return captured exit code.

Both flows must return `ExecResult` with explicit exit code and map missing
exit status to a semantic `ExecFailed` error.

### Stage C: Handle terminal resize (`SIGWINCH`) for interactive mode

Add resize handling for attached TTY sessions:

- subscribe to `SIGWINCH` (Unix build target);
- capture current terminal rows/columns via a dedicated helper seam;
- call Bollard resize for the active exec id;
- stop listener cleanly when exec session ends.

Non-interactive and detached runs must not spawn resize listeners.

If resize support on non-Unix targets requires materially different behaviour,
keep Unix implementation primary and document the platform limitation in
`docs/users-guide.md` and `docs/podbot-design.md`.

### Stage D: Wire CLI execution path and exit behaviour

Update CLI surface and dispatch:

- extend `ExecArgs` in `src/config/cli.rs` to represent attached vs detached
  mode while preserving current command form;
- replace `exec_in_container` stub in `src/main.rs` with real execution wiring:
  resolve socket, connect engine, execute request, render result.

Exit-code behaviour contract:

- success exit code `0` returns `Ok(())`;
- non-zero command exit code is surfaced accurately to the caller (via explicit
  CLI propagation strategy selected in Decision log), with tests proving
  correctness.

### Stage E: Unit tests (`rstest`) for happy, unhappy, and edge cases

Add/extend unit tests around the new exec module, using fixtures and mock
traits.

Required coverage:

- happy: attached exec with tty and exit code `0`;
- happy: detached exec with exit code `0`;
- unhappy: engine create/start failure maps to `ExecFailed`;
- unhappy: inspect failure maps to `ExecFailed`;
- unhappy: missing exit code maps to `ExecFailed`;
- edge: empty/whitespace command rejected before engine call;
- edge: resize event triggers resize call with expected dimensions;
- edge: no resize listener for detached or non-tty mode.

### Stage F: Behavioural tests (`rstest-bdd` v0.5.0)

Add a dedicated feature and scenario suite for interactive exec:

- `tests/features/interactive_exec.feature`
- `tests/bdd_interactive_exec.rs`
- `tests/bdd_interactive_exec_helpers/{mod,state,steps,assertions}.rs`

Scenario matrix:

- attached interactive exec succeeds and reports exit code `0`;
- attached interactive exec returns non-zero exit code accurately;
- detached exec succeeds and reports exit code;
- resize event during attached session triggers resize propagation;
- daemon/exec failure returns actionable `ExecFailed` message.

Keep helpers split to avoid oversized files and reuse existing BDD state/step
patterns from engine/container suites.

### Stage G: Documentation and roadmap updates

Update `docs/podbot-design.md` with implementation decisions for:

- exec request model and mode semantics;
- resize propagation strategy;
- exit-code capture and propagation behaviour.

Update `docs/users-guide.md` for user-visible changes in `podbot exec`:

- attached vs detached usage;
- tty behaviour;
- resize behaviour expectations;
- exit-code behaviour and troubleshooting.

After code/tests/docs pass all gates, mark Step 2.4 tasks done in
`docs/podbot-roadmap.md`.

### Stage H: Verification gates and evidence

Run required gates with log capture via `tee`:

```sh
set -o pipefail
make check-fmt 2>&1 | tee "/tmp/check-fmt-$(get-project)-$(git branch --show).out"
make lint 2>&1 | tee "/tmp/lint-$(get-project)-$(git branch --show).out"
make test 2>&1 | tee "/tmp/test-$(get-project)-$(git branch --show).out"
```

If any gate fails, fix and rerun until all three pass, preserving failed-run
logs for auditability.

## Validation and acceptance

Acceptance is met only when all are true:

- Roadmap Step 2.4 checkboxes in `docs/podbot-roadmap.md` are all done.
- `podbot exec` supports both attached and detached modes.
- Attached interactive sessions propagate `SIGWINCH` resize updates.
- Executed command exit codes are captured and returned accurately.
- Unit tests (`rstest`) and behavioural tests (`rstest-bdd` v0.5.0) cover happy,
  unhappy, and edge paths.
- `docs/podbot-design.md` records final design choices.
- `docs/users-guide.md` reflects user-visible exec behaviour.
- `make check-fmt`, `make lint`, and `make test` pass.

## Idempotence and recovery

- Each stage is additive and can be rerun safely.
- If partial edits leave the tree failing, revert only incomplete hunks and
  replay the current stage.
- Keep gate logs per run under `/tmp` with unique action names.
- Re-run full gates on final tip before marking roadmap items done.

## Progress

- [x] (2026-02-22 UTC) Confirmed branch context and validated requested
      reference documents exist.
- [x] (2026-02-22 UTC) Reviewed roadmap/design/testing guidance and current
      exec stub/engine module boundaries.
- [x] (2026-02-22 UTC) Drafted this ExecPlan at
      `docs/execplans/2-4-1-interactive-execution.md`.
- [x] (2026-02-22 UTC) Implemented Stage A with new exec module, typed request/
      result types, and a mockable `ContainerExecClient` seam.
- [x] (2026-02-22 UTC) Implemented Stage B attached/detached execution flows
      with create/start/inspect lifecycle management and exit-code capture.
- [x] (2026-02-22 UTC) Implemented Stage C terminal resize behaviour with
      initial resize and Unix `SIGWINCH` propagation for attached TTY sessions.
- [x] (2026-02-22 UTC) Implemented Stage D CLI wiring for `podbot exec`,
      including detached mode and process exit-code propagation.
- [x] (2026-02-22 UTC) Implemented Stage E `rstest` unit tests for happy,
      unhappy, and edge paths in `src/engine/connection/exec/tests.rs`.
- [x] (2026-02-22 UTC) Implemented Stage F `rstest-bdd` behavioural scenarios
      in `tests/features/interactive_exec.feature` and helper modules.
- [x] (2026-02-22 UTC) Implemented Stage G documentation updates in
      `docs/podbot-design.md`, `docs/users-guide.md`, and roadmap completion in
      `docs/podbot-roadmap.md`.
- [x] (2026-02-22 UTC) Completed Stage H gate stack with passing runs logged at
      `/tmp/check-fmt-podbot-2-4-1-interactive-execution.out`,
      `/tmp/lint-podbot-2-4-1-interactive-execution.out`, and
      `/tmp/test-podbot-2-4-1-interactive-execution.out`.

## Surprises and discoveries

- `src/main.rs` currently keeps `exec_in_container` as a stub, so Step 2.4
  requires first real command-execution wiring in the binary path.
- The engine module already uses trait seams for daemon-independent unit tests
  in create/upload flows, so Step 2.4 should mirror that pattern rather than
  introducing integration-test-only coverage.
- The workspace hit a transient `No space left on device` failure during test
  file creation; `cargo clean` recovered enough space and work resumed without
  code changes.

## Decision log

- Decision: define detached mode as no stream attachment but still wait for and
  report exit code. Rationale: satisfies both detached execution support and
  explicit exit-code completion criteria in Step 2.4.
- Decision: implement resize handling only for attached TTY sessions.
  Rationale: detached or non-tty execution has no interactive terminal surface
  to resize.
- Decision: maintain existing command form `podbot exec <container> -- <command`
  `...>` and add mode control additively. Rationale: preserves backward
  compatibility while enabling detached mode.
- Decision: use `stty size` behind a small `TerminalSizeProvider` seam for
  terminal dimensions. Rationale: keeps production logic simple and testable
  without coupling tests to host terminal state.
- Decision: treat daemon completion without `exit_code` as
  `ContainerError::ExecFailed`. Rationale: avoids silently fabricating success
  or failure for ambiguous daemon responses.

## Outcomes and retrospective

Shipped:

- New exec lifecycle support in `EngineConnector` with attached and detached
  modes, stream forwarding, resize propagation, and exit-code capture.
- CLI `exec` now supports `--detach` and propagates command exit status to the
  podbot process exit code.
- Unit and behavioural test coverage for happy/unhappy/edge paths using
  `rstest` and `rstest-bdd` v0.5.0.
- Design, user, and roadmap docs updated to match implemented behaviour.

Risk outcomes:

- Terminal resize complexity was contained by splitting logic into
  `attached.rs` and `terminal.rs`.
- Stream forwarding deadlock risk was mitigated by explicit stdin-task abort and
  join handling after output-stream completion.
- Detached semantics ambiguity was resolved by explicit wait-for-exit behaviour
  and documented in user/design docs.

Follow-up beyond Step 2.4:

- None required for roadmap Step 2.4 completion.
