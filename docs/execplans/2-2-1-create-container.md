# Step 2.2.1: Create container with configurable security options

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE

No `PLANS.md` file exists in this repository as of 2026-02-10, so this ExecPlan
is the governing implementation document for this task.

## Purpose and big picture

Implement the Step 2.2 container-creation slice from `docs/podbot-roadmap.md`
by adding a Bollard-backed `create_container` capability that applies sandbox
security settings deterministically.

After this change, callers can request container creation with explicit
security options and receive a container identifier on success. The behaviour
is observable through unit tests and behaviour tests that validate both happy
paths and failure modes without requiring a live daemon for every case.

This plan also covers required documentation updates:

- Record security-profile design decisions in `docs/podbot-design.md`.
- Update user-facing behaviour in `docs/users-guide.md`.
- Mark the relevant roadmap entry as done once acceptance criteria are met.

## Constraints

- Keep existing socket-resolution and health-check behaviour unchanged.
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
- Use en-GB-oxendict spelling in documentation updates.
- Prefer Makefile targets for verification (`make check-fmt`, `make lint`,
  `make test`).

## Tolerances (exception triggers)

- Scope: if implementation requires edits in more than 12 files or more than
  500 net lines, stop and confirm scope.
- Public API: if an existing public API must break compatibility, stop and
  confirm the migration strategy.
- Dependencies: if a new dependency is required for mocking or container
  modelling, stop and confirm before adding it.
- Ambiguity: if SELinux handling cannot be implemented safely from current
  requirements, stop and present concrete options.
- Iterations: if `make lint` or `make test` still fails after two fix passes,
  stop and document the blocker with log evidence.

## Risks

- Risk: Bollard type surfaces for `HostConfig` and device mapping are verbose,
  which can increase cognitive complexity. Severity: medium Likelihood: medium
  Mitigation: isolate translation logic in focused helper functions with
  predicate helpers.

- Risk: Security options suitable for SELinux vary by engine/runtime context.
  Severity: medium Likelihood: medium Mitigation: encode explicit defaults with
  tests and document rationale plus limits in the design document.

- Risk: Behaviour tests may become flaky if they depend on a real daemon.
  Severity: medium Likelihood: low Mitigation: use dependency injection with
  deterministic test doubles for most scenarios, and skip only where runtime
  availability is inherently external.

- Risk: Existing engine module can exceed size guidance.
  Severity: low Likelihood: high Mitigation: introduce a dedicated
  container-creation submodule instead of appending all logic to
  `src/engine/connection/mod.rs`.

## Progress

- [x] (2026-02-10 UTC) Reviewed roadmap, design, testing guides, and current
      engine code structure.
- [x] (2026-02-10 UTC) Drafted this ExecPlan at
      `docs/execplans/2-2-1-create-container.md`.
- [x] (2026-02-11 UTC) Implemented container-creation security modelling and
      Bollard request construction.
- [x] (2026-02-11 UTC) Added `rstest` unit tests for happy, unhappy, and edge
      paths.
- [x] (2026-02-11 UTC) Added `rstest-bdd` v0.5.0 scenarios for create-container
      behaviour.
- [x] (2026-02-11 UTC) Updated design and user documentation.
- [x] (2026-02-11 UTC) Marked the relevant roadmap entry as done.
- [x] (2026-02-11 UTC) Ran quality gates and captured logs.

## Surprises and discoveries

- Observation: container lifecycle operations are not yet implemented; only
  engine connection and health-check paths exist in `src/engine/connection`.
  Evidence: `src/engine/mod.rs` exports only `EngineConnector` and
  `SocketResolver`, with no create/start/upload/exec methods. Impact: this task
  will introduce the first lifecycle operation and should establish a reusable
  structure for Steps 2.3 and 2.4.

- Observation: `ContainerError` already includes `CreateFailed`, which reduces
  error-surface churn for this task. Evidence: `src/error.rs` defines
  `ContainerError::CreateFailed`. Impact: implementation can map Bollard create
  failures directly without adding new top-level error variants.

## Decision log

- Decision: scope this ExecPlan to Step 2.2.1 (create-container implementation)
  while designing interfaces that do not block Step 2.2 follow-on tasks.
  Rationale: the request targets `2-2-1-create-container.md`, but the interface
  should remain suitable for privileged/minimal/SELinux/image requirements in
  the same phase. Date/Author: 2026-02-10 / Codex

- Decision: require deterministic test coverage through dependency injection for
  create-container request building and error mapping. Rationale: this aligns
  with `docs/reliable-testing-in-rust-via-dependency- injection.md` and avoids
  brittle daemon-dependent tests for core logic. Date/Author: 2026-02-10 / Codex

- Decision: introduce a public `ContainerCreator` abstraction and explicit
  `CreateContainerRequest`/`ContainerSecurityOptions` types in `src/engine`.
  Rationale: this supports deterministic unit and behavioural tests without a
  running daemon while preserving a stable seam for Step 2.3 and Step 2.4.
  Date/Author: 2026-02-11 / Codex

## Outcomes and retrospective

Implemented Step 2.2.1 container creation with configurable security options.

- Added `EngineConnector::create_container_async` and synchronous
  `EngineConnector::create_container`.
- Added public engine types: `ContainerCreator`, `CreateContainerRequest`,
  `ContainerSecurityOptions`, and `SelinuxLabelMode`.
- Implemented privileged and minimal security mapping, optional `/dev/fuse`
  handling, and SELinux label controls.
- Added semantic missing-image validation via `ConfigError::MissingRequired`.
- Added unit and behavioural coverage for happy, unhappy, and edge paths.
- Updated `docs/podbot-design.md`, `docs/users-guide.md`, and
  `docs/podbot-roadmap.md`.

This milestone stops at container creation; start/execution lifecycle work
remains in subsequent roadmap steps.

## Context and orientation

Current engine code is organised under `src/engine/connection/` and supports:

- socket resolution from config, environment, and defaults;
- connection establishment via Bollard;
- health-check verification with timeout handling.

Relevant files for this work:

- `src/engine/mod.rs`
- `src/engine/connection/mod.rs`
- `src/engine/connection/health_check.rs`
- `src/error.rs`
- `src/config/types.rs`
- `tests/features/engine_connection.feature`
- `tests/bdd_engine_connection.rs`
- `tests/bdd_engine_connection_helpers/`
- `docs/podbot-design.md`
- `docs/users-guide.md`
- `docs/podbot-roadmap.md`

Key requirement translation for Step 2.2.1:

- Add `create_container` implementation that accepts configurable security
  options.
- Support privileged and minimal security profiles.
- Support `/dev/fuse` mounting control.
- Apply SELinux-compatible security options in a deterministic way.
- Use configured image input.

## Plan of work

### Stage A: Introduce container-creation module and API surface

Create a dedicated lifecycle submodule under `src/engine/` to keep complexity
bounded and make later Steps 2.3/2.4 additive.

Add a focused API that separates:

- request input (`image`, sandbox/security profile, optional container name,
  command/environment inputs if required);
- security translation (sandbox config to Bollard `HostConfig` fields);
- engine operation invocation (`docker.create_container`).

Target outcome: a `create_container` function that returns container ID on
success and maps errors to `ContainerError::CreateFailed`.

Validation gate:

- new module compiles;
- no behavioural changes to existing connection tests.

### Stage B: Implement security-profile translation logic

Add helper functions that build HostConfig for the two supported modes:

- privileged mode (`sandbox.privileged = true`);
- minimal mode (`sandbox.privileged = false`) with optional `/dev/fuse` device
  mapping controlled by `sandbox.mount_dev_fuse`.

Include SELinux-oriented security options in the translation layer and codify
that behaviour with tests and design-document rationale.

Target outcome: deterministic, testable mapping from config/security options to
Bollard request payload.

Validation gate:

- unit tests validate generated HostConfig for happy and edge permutations.

### Stage C: Add unit tests with rstest

Implement module-local unit tests using `rstest` fixtures and parameterized
cases. Coverage must include:

- happy path: privileged profile request creation;
- happy path: minimal profile with `/dev/fuse` enabled;
- edge path: minimal profile with `/dev/fuse` disabled;
- unhappy path: missing image input returns semantic configuration error;
- unhappy path: Bollard create failure maps to `ContainerError::CreateFailed`.

Prefer deterministic test doubles for engine call boundaries.

Validation gate:

- `cargo test` for the targeted module passes before proceeding.

### Stage D: Add behavioural tests with rstest-bdd v0.5.0

Add a dedicated feature file and scenario bindings for container creation,
covering observable behaviour in Given-When-Then form.

Proposed scenarios:

- create succeeds in privileged mode;
- create succeeds in minimal mode with `/dev/fuse`;
- create fails when image is not configured;
- create surfaces engine failure as create-container error.

Use state fixtures and step helpers in a new helper module mirroring the
existing engine-connection BDD structure.

Validation gate:

- new behavioural tests pass under `make test` with `rstest-bdd` macros.

### Stage E: Documentation and roadmap updates

Update `docs/podbot-design.md` with a concise subsection documenting:

- chosen security profile mapping;
- SELinux option rationale and constraints;
- relationship between config fields and container HostConfig.

Update `docs/users-guide.md` with user-visible behaviour for container
creation, including how sandbox settings influence runtime security and
compatibility.

Update `docs/podbot-roadmap.md` by marking the relevant Step 2.2 entry done
once tests and quality gates pass.

Validation gate:

- documentation is accurate to implemented behaviour;
- roadmap checkbox state matches delivered scope.

### Stage F: Quality gates and commit

Run required quality gates and retain logs, then commit atomically with a
message that explains what changed and why.

Validation gate:

- `make check-fmt`, `make lint`, `make test` all exit zero;
- commit includes only intended files.

## Concrete steps

All commands run from repository root: `/data/leynos/Projects/podbot`.

1. Implement lifecycle module and wire exports.
2. Implement `create_container` and security translation helpers.
3. Add `rstest` unit tests for request construction and failure mapping.
4. Add `rstest-bdd` scenarios, bindings, and step helpers for behaviour tests.
5. Update design and users guide documents.
6. Mark roadmap entry complete.
7. Run verification with log capture:

    set -o pipefail
    make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-$(git branch --show).out

    set -o pipefail
    make lint 2>&1 | tee /tmp/lint-podbot-$(git branch --show).out

    set -o pipefail
    make test 2>&1 | tee /tmp/test-podbot-$(git branch --show).out

8. Review diffs and commit.

## Validation and acceptance

Implementation is complete when all conditions below are true:

- A callable `create_container` path exists in the engine wrapper and returns a
  container ID on success.
- Security options are configurable and covered by tests for privileged,
  minimal, and fuse-disabled permutations.
- Unhappy paths are covered by tests, including image-missing and engine
  failure cases.
- Behaviour tests implemented with `rstest-bdd` v0.5.0 pass.
- `docs/podbot-design.md` records design decisions for container security
  mapping.
- `docs/users-guide.md` describes user-visible behaviour changes.
- `docs/podbot-roadmap.md` marks the relevant Step 2.2 entry done.
- `make check-fmt`, `make lint`, and `make test` all pass.

## Idempotence and recovery

- Code-edit stages are safe to rerun; repeated edits should converge on the
  same end state.
- If test failures occur, capture output logs, fix one class of failure at a
  time, and rerun the specific failing command.
- If a tolerance trigger is hit, stop implementation, update `Decision log`,
  and wait for direction before expanding scope.

## Artifacts and notes

Expected verification artifacts:

- `/tmp/check-fmt-podbot-<branch>.out`
- `/tmp/lint-podbot-<branch>.out`
- `/tmp/test-podbot-<branch>.out`

Keep these paths in the final implementation summary for traceability.

## Interfaces and dependencies

Planned interfaces (names may be refined during implementation, but intent is
fixed):

- engine-layer API for creating containers from explicit security/image inputs;
- helper for translating sandbox configuration into Bollard host-security
  fields;
- deterministic test seam (trait or adapter) so request construction and error
  mapping are testable without a live daemon.

Dependencies remain unchanged unless a tolerance trigger is explicitly invoked.

## Revision note

Initial draft created on 2026-02-10 to plan Step 2.2.1 container creation work,
including implementation, testing, documentation, and roadmap-completion tasks.

Revised on 2026-02-11 to record delivered implementation, decisions, and
quality-gated completion evidence.
