# Step 2.2.2: Support privileged mode for maximum compatibility

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE

No `PLANS.md` file exists in this repository as of 2026-02-12, so this ExecPlan
is the governing implementation document for this task.

## Purpose and big picture

Complete the Step 2.2.2 roadmap item: "Support privileged mode for maximum
compatibility." The goal is to ensure that when `sandbox.privileged = true`,
the container creation path produces a fully correct privileged-mode
`HostConfig`, that the configuration flows end-to-end from `SandboxConfig`
through `ContainerSecurityOptions` to the Bollard create payload, and that this
behaviour is explicitly validated with dedicated unit tests and
behaviour-driven development (BDD) scenarios focused on the privileged-mode
contract.

After this change, a user who sets `sandbox.privileged = true` in their
configuration can be confident that:

- the container runs with `HostConfig.Privileged = true`;
- no additional capabilities, device mappings, or SELinux overrides are applied
  (the engine host profile governs these);
- the `mount_dev_fuse` and `selinux_label_mode` settings are intentionally
  ignored in privileged mode because the engine provides full device and
  security access;
- this behaviour is observable via `make test`.

Step 2.2.1 introduced the container-creation infrastructure and included
initial test coverage for privileged mode as part of the broader security
mapping. This task validates that coverage is comprehensive by adding edge-case
unit tests and dedicated BDD scenarios that focus specifically on the
privileged-mode contract, including interactions with other sandbox settings
that should be ignored.

## Constraints

- Keep existing socket-resolution and health-check behaviour unchanged.
- Do not modify the existing container-creation public API surface unless a
  defect is discovered.
- Do not add new third-party dependencies unless a blocker is documented in the
  `Decision log` and approved.
- Keep module-level `//!` documentation in every Rust module touched.
- Avoid `unwrap` and `expect` outside test code.
- Use `rstest` fixtures for unit tests and `rstest-bdd` v0.5.0 for behavioural
  tests.
- Keep files under 400 lines where practical; split modules when a file would
  exceed this limit.
- Use en-GB-oxendict spelling in documentation updates.
- Prefer Makefile targets for verification (`make check-fmt`, `make lint`, and
  `make test`).
- Existing tests must continue to pass throughout implementation.

## Tolerances (exception triggers)

- Scope: if implementation requires edits in more than 10 files or more than
  300 net lines, stop and confirm scope.
- Public API: if an existing public API signature must change, stop and confirm
  the migration strategy.
- Dependencies: if a new dependency is required, stop and confirm before adding
  it.
- Iterations: if `make lint` or `make test` still fails after two fix passes,
  stop, and document the blocker with log evidence.
- Ambiguity: if the privileged-mode semantics require reinterpretation of
  existing design-document wording, stop and present options.

## Risks

- Risk: the privileged-mode code path is already largely implemented in 2.2.1,
  so the scope of new code may be small. Severity: low. Likelihood: high.
  Mitigation: focus the work on edge-case coverage, explicit validation of the
  "ignored settings" contract, and documentation clarity. The value is in
  rigorous validation, not new features.

- Risk: adding edge-case tests might reveal that the existing
  `build_host_config` function needs adjustment for combinations not yet
  exercised. Severity: medium. Likelihood: low. Mitigation: run existing tests
  first to confirm a green baseline, then add tests one at a time.

- Risk: BDD test file sizes may approach the 400-line limit with additional
  scenarios. Severity: low. Likelihood: medium. Mitigation: keep assertion
  helpers factored in the existing `assertions.rs` module and keep step
  definitions focused.

## Progress

- [x] (2026-02-12 UTC) Review existing implementation and confirm green
      baseline.
- [x] (2026-02-12 UTC) Add edge-case unit tests for privileged mode.
- [x] (2026-02-12 UTC) Add BDD scenarios focused on privileged-mode contract.
- [x] (2026-02-12 UTC) Update design documentation if needed.
- [x] (2026-02-12 UTC) Update user's guide if needed.
- [x] (2026-02-12 UTC) Mark roadmap entry done.
- [x] (2026-02-12 UTC) Run quality gates and capture logs.

## Surprises and discoveries

- Observation: the privileged-mode code path was already fully functional from
  Step 2.2.1, including the `build_host_config` branching logic. Evidence:
  existing test `create_container_privileged_mode_has_minimal_overrides`
  already validated the happy path. Impact: the scope of this task was
  primarily edge-case validation and documentation, not new feature
  implementation.

## Decision log

- Decision: scope this ExecPlan to validate and harden the privileged-mode code
  path with additional edge-case tests and BDD scenarios, rather than
  reimplementing the privileged-mode logic which already exists. Rationale:
  Step 2.2.1 already implemented the core `build_host_config` branching for
  `security.privileged == true` in
  `src/engine/connection/create_container/mod.rs` (lines 272–281). The roadmap
  item 2.2.2 asks to "support privileged mode for maximum compatibility", and
  the value remaining is in rigorous validation of edge cases and
  documentation. Date/Author: 2026-02-12 / Claude.

## Outcomes and retrospective

Implemented Step 2.2.2 privileged mode validation and documentation.

- Added four edge-case unit tests validating that privileged mode ignores
  `mount_dev_fuse` and `selinux_label_mode` settings, that
  `from_sandbox_config` correctly derives SELinux mode for both privileged and
  non-privileged paths, and that privileged mode works without optional fields.
- Added two BDD scenarios confirming privileged mode ignores `/dev/fuse` and
  SELinux override settings from a user-observable perspective.
- Updated `docs/podbot-design.md` with explicit documentation that privileged
  mode ignores FUSE and SELinux settings.
- Updated `docs/users-guide.md` noting that `mount_dev_fuse` has no effect in
  privileged mode.
- Marked roadmap entry 2.2.2 as done.
- All quality gates pass: `make check-fmt`, `make lint`, `make test`.

The privileged-mode code path was already implemented in Step 2.2.1. This task
confirmed correctness through rigorous edge-case validation and improved
documentation clarity.

## Context and orientation

The container-creation module lives at
`src/engine/connection/create_container/mod.rs` (313 lines). It contains:

- `ContainerSecurityOptions` struct (lines 64–73): holds `privileged`,
  `mount_dev_fuse`, and `selinux_label_mode` fields.
- `ContainerSecurityOptions::from_sandbox_config` (lines 78–91): translates
  `SandboxConfig` into security options. When `sandbox.privileged = true`,
  SELinux mode is set to `KeepDefault`.
- `build_host_config` (lines 272–295): the core translation function. When
  `security.privileged == true`, it returns a `HostConfig` with only
  `privileged: Some(true)` set and all other fields defaulted. When
  `security.privileged == false`, it applies the minimal profile with
  conditional FUSE and SELinux settings.
- `CreateContainerRequest` builder (lines 104–195): holds image, name, cmd,
  env, and security options.
- `EngineConnector::create_container_async` (lines 204–221) and synchronous
  wrapper (lines 232–238).

Unit tests live at `src/engine/connection/create_container/tests.rs` (341
lines). Existing relevant tests:

- `from_sandbox_config_preserves_flags` — validates config-to-options mapping
  with `privileged: true`.
- `create_container_privileged_mode_has_minimal_overrides` — validates that the
  privileged path sets `privileged: Some(true)` and leaves `cap_add`,
  `devices`, and `security_opt` as `None`.

BDD tests live at:

- `tests/features/container_creation.feature` — includes "Create container in
  privileged mode" scenario.
- `tests/bdd_container_creation.rs` — scenario bindings.
- `tests/bdd_container_creation_helpers/` — state, steps, and assertions.

Configuration types at `src/config/types.rs` define `SandboxConfig` with
`privileged: bool` (default `false`) and `mount_dev_fuse: bool` (default
`true`).

## Plan of work

### Stage A: Confirm green baseline (no code changes)

Run `make check-fmt`, `make lint`, and `make test` to confirm the existing
codebase passes all quality gates before any changes.

Validation gate: all three commands exit zero.

### Stage B: Add edge-case unit tests for privileged mode

Add unit tests in `src/engine/connection/create_container/tests.rs` that
specifically exercise the privileged-mode contract under edge-case
configurations:

1. **Privileged mode ignores `mount_dev_fuse = false`**: create a request with
   `privileged: true` and `mount_dev_fuse: false`. Verify that the resulting
   `HostConfig` still has only `privileged: Some(true)` with no devices,
   capabilities, or SELinux overrides. This confirms that the FUSE toggle is
   intentionally irrelevant in privileged mode.

2. **Privileged mode ignores `DisableForContainer` SELinux mode**: create a
   request with `privileged: true` and
   `selinux_label_mode: DisableForContainer`. Verify that `security_opt` is
   `None`, confirming the SELinux setting is ignored in privileged mode.

3. **`from_sandbox_config` for non-privileged mode**: verify that
   `from_sandbox_config` with `privileged: false` produces
   `SelinuxLabelMode::DisableForContainer` (confirming the inverse case).

4. **Privileged mode with no name, cmd, or env**: verify the created body
   has the correct image and host config with `None` for optional fields.

Validation gate: `make test` passes with the new tests.

### Stage C: Add BDD scenarios for privileged-mode edge cases

Extend `tests/features/container_creation.feature` with additional scenarios
that validate privileged-mode behaviour from a user-observable perspective:

1. **Privileged mode ignores /dev/fuse setting**: Given privileged mode is
   enabled and `/dev/fuse` mounting is explicitly disabled, when container
   creation is requested, then the container is created in privileged mode
   without FUSE-specific overrides.

2. **Privileged mode with explicit SELinux disable**: Given privileged mode is
   enabled and SELinux label disable is requested, when container creation is
   requested, then privileged configuration is used and the SELinux setting is
   ignored.

Add the corresponding step definitions and assertion helpers in the existing
BDD helper modules.

Validation gate: `make test` passes with the new scenarios.

### Stage D: Documentation updates

Review and update documentation as needed:

- `docs/podbot-design.md`: verify the container-creation security mapping
  section clearly documents that privileged mode intentionally ignores
  `mount_dev_fuse` and SELinux settings. Add a sentence if this is not already
  explicit.

- `docs/users-guide.md`: verify the sandbox configuration section clearly
  explains the privileged-mode trade-off and documents that `mount_dev_fuse`
  has no effect in privileged mode.

- `docs/podbot-roadmap.md`: mark task 2.2.2 as done by changing
  `[ ] Support privileged mode for maximum compatibility.` to
  `[x] Support privileged mode for maximum compatibility.`

Validation gate: documentation accurately describes implemented behaviour; no
misleading or ambiguous wording remains.

### Stage E: Quality gates and commit

Run required quality gates and retain logs, then commit atomically.

Validation gate: `make check-fmt`, `make lint`, and `make test` all exit zero.

## Concrete steps

All commands run from repository root: `/data/leynos/Projects/podbot`.

1. Confirm green baseline:

   ```bash
   make check-fmt && make lint && make test
   ```

2. Add edge-case unit tests in
   `src/engine/connection/create_container/tests.rs`.

3. Add BDD scenarios in `tests/features/container_creation.feature` and
   corresponding step definitions and assertions in
   `tests/bdd_container_creation_helpers/`.

4. Add scenario bindings in `tests/bdd_container_creation.rs`.

5. Review and update `docs/podbot-design.md`, `docs/users-guide.md`, and
   `docs/podbot-roadmap.md`.

6. Run verification with log capture:

   ```bash
   set -o pipefail
   make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-2-2-2.out
   ```

   ```bash
   set -o pipefail
   make lint 2>&1 | tee /tmp/lint-podbot-2-2-2.out
   ```

   ```bash
   set -o pipefail
   make test 2>&1 | tee /tmp/test-podbot-2-2-2.out
   ```

7. Review diffs and commit.

## Validation and acceptance

Implementation is complete when all conditions below are true:

- Edge-case unit tests validate that privileged mode ignores `mount_dev_fuse`
  and `selinux_label_mode` settings.
- BDD scenarios validate privileged-mode behaviour from a user perspective.
- `docs/podbot-design.md` explicitly documents that `mount_dev_fuse` and
  SELinux settings are ignored in privileged mode.
- `docs/users-guide.md` documents privileged-mode behaviour clearly.
- `docs/podbot-roadmap.md` marks task 2.2.2 as done.
- `make check-fmt`, `make lint`, and `make test` all pass.

Quality criteria (what "done" means):

- Tests: all existing and new tests pass.
- Lint/typecheck: `make lint` passes with `-D warnings`.
- Formatting: `make check-fmt` passes.

Quality method (how we check):

- Run `make check-fmt && make lint && make test` and verify zero exit codes.

## Idempotence and recovery

- Code-edit stages are safe to rerun; repeated edits should converge on the
  same end state.
- If test failures occur, capture output logs, fix one class of failure at a
  time, and rerun the specific failing command.
- If a tolerance trigger is hit, stop implementation, update `Decision log`,
  and wait for direction before expanding scope.

## Artefacts and notes

Expected verification artefacts:

- `/tmp/check-fmt-podbot-2-2-2.out`
- `/tmp/lint-podbot-2-2-2.out`
- `/tmp/test-podbot-2-2-2.out`

## Interfaces and dependencies

No new interfaces or dependencies are required. This task validates the
existing interface defined in Step 2.2.1:

- `ContainerSecurityOptions` — unchanged.
- `CreateContainerRequest::new()` — unchanged.
- `EngineConnector::create_container_async()` — unchanged.
- `build_host_config()` (internal) — unchanged unless edge-case testing reveals
  a defect.
- `ContainerCreator` trait and `MockCreator` — reused from existing tests and
  BDD helpers.
