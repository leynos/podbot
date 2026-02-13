# Step 2.2.3: Support minimal mode with only /dev/fuse mounted

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE

No `PLANS.md` file exists in this repository as of 2026-02-13, so this ExecPlan
is the governing implementation document for this task.

## Purpose and big picture

Complete the Step 2.2.3 roadmap item: "Support minimal mode with only /dev/fuse
mounted." The goal is to ensure that when `sandbox.privileged = false` (the
default), the container creation path produces a correctly configured minimal
`HostConfig` that:

- sets `HostConfig.Privileged = false`;
- applies `SecurityOpt = ["label=disable"]` so rootless nested Podman workflows
  succeed under strict Security-Enhanced Linux (SELinux) labelling;
- when `sandbox.mount_dev_fuse = true` (the default), maps `/dev/fuse` with
  `rwm` permissions and adds the `SYS_ADMIN` capability to support
  `fuse-overlayfs`;
- when `sandbox.mount_dev_fuse = false`, omits the `/dev/fuse` device mapping
  and the `SYS_ADMIN` capability addition.

After this change, a user who accepts the defaults
(`sandbox.privileged = false`, `sandbox.mount_dev_fuse = true`) can be
confident that:

- the container provides the minimum security surface needed for inner Podman
  execution via fuse-overlayfs;
- SELinux label isolation is disabled at the container process level to avoid
  labelling failures;
- these behaviours are explicitly validated by unit and behavioural tests;
- the user's guide documents the minimal mode behaviour clearly;
- this behaviour is observable via `make test`.

Step 2.2.1 introduced the core container-creation infrastructure, and Step
2.2.2 added dedicated edge-case coverage for the privileged-mode path.
Examination of the existing code reveals that the minimal-mode code path in
`build_host_config` (`src/engine/connection/create_container/mod.rs`) already
implements the required behaviour. Similarly, existing unit tests
(`create_container_minimal_mode_mounts_fuse`,
`create_container_minimal_without_fuse_avoids_mount`) and behaviour-driven
development (BDD) scenarios ("Create container in minimal mode with /dev/fuse",
"Create container in minimal mode without /dev/fuse") already cover the primary
happy paths.

The remaining work for this task is therefore to:

1. Add dedicated edge-case unit tests for the minimal-mode code path, mirroring
   the thoroughness applied to the privileged-mode path in Step 2.2.2.
2. Add BDD scenarios that cover minimal-mode-specific edge cases.
3. Ensure the design document accurately describes the minimal-mode behaviour.
4. Ensure the user's guide documents minimal mode clearly for end users.
5. Mark the roadmap entry as done.

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
  stop and document the blocker with log evidence.
- Ambiguity: if the minimal-mode semantics require reinterpretation of existing
  design-document wording, stop and present options.

## Risks

- Risk: the minimal-mode code path is already implemented in Step 2.2.1, so
  the scope of new code is small. Severity: low. Likelihood: high. Mitigation:
  focus work on edge-case coverage, explicit validation, and documentation
  clarity. The value is in rigorous validation, not new features.

- Risk: adding dedicated edge-case tests for minimal mode may push test files
  over 400 lines. Severity: low. Likelihood: medium. Mitigation: split
  minimal-mode edge-case tests into a dedicated submodule (mirroring the
  `privileged_mode.rs` pattern).

## Progress

- [x] (2026-02-13) Read and understand existing implementation and tests.
- [x] (2026-02-13) Add minimal-mode edge-case unit tests in a dedicated
  submodule (`minimal_mode.rs` with 5 tests).
- [x] (2026-02-13) Add minimal-mode-specific BDD scenarios and step
  definitions (2 new scenarios, 1 new Given step, 2 new Then assertions).
- [x] (2026-02-13) Verify design document describes minimal-mode behaviour
  accurately. Confirmed: "Container creation security mapping" section in
  `podbot-design.md` is accurate; no changes needed.
- [x] (2026-02-13) Verify user's guide documents minimal mode clearly.
  Confirmed: "Sandbox configuration" section in `users-guide.md` accurately
  describes minimal mode; no changes needed.
- [x] (2026-02-13) Run quality gates (`make check-fmt`, `make lint`,
  `make test`). All pass.
- [x] (2026-02-13) Mark roadmap entry 2.2.3 as done.

## Surprises and discoveries

- Observation: The minimal-mode implementation was fully complete as of Step
  2.2.1. The entire `build_host_config` function already handles all four
  permutations (privileged/non-privileged, fuse on/off). Evidence: Code review
  of `build_host_config` and existing test coverage. Impact: The scope of this
  task was purely additive test coverage and documentation validation, with no
  production code changes required.

## Decision log

- Decision: Follow the same structural pattern as Step 2.2.2 (privileged mode),
  creating a `minimal_mode.rs` submodule for edge-case unit tests. Rationale:
  Consistency with existing code structure; keeps `mod.rs` under 400 lines.
  Date/Author: 2026-02-13, agent.

- Decision: Existing implementation is sufficient; no code changes to
  `build_host_config` or `ContainerSecurityOptions` are required. Rationale:
  Examination of `build_host_config` confirms it already handles all four
  minimal-mode permutations (fuse on/off, SELinux disable). The function was
  implemented in Step 2.2.1. Date/Author: 2026-02-13, agent.

## Outcomes and retrospective

The task is complete. Five new edge-case unit tests and two new BDD scenarios
were added to validate the minimal-mode container-creation path. No production
code changes were required because the implementation was already complete from
Step 2.2.1.

New test coverage added:

- Unit tests (in `minimal_mode.rs`):
  - `from_sandbox_config_minimal_mode_sets_selinux_disable`
  - `create_container_minimal_mode_with_selinux_keep_default`
  - `create_container_minimal_mode_without_fuse_omits_capabilities`
  - `create_container_minimal_mode_with_fuse_verifies_device_details`
  - `create_container_minimal_mode_without_optional_fields`

- BDD scenarios (in `container_creation.feature`):
  - "Minimal mode with SELinux kept at default"
  - "Minimal mode without /dev/fuse omits capabilities"

Lessons learned:

- When implementation is completed in an earlier step, the follow-up task
  becomes a validation and documentation exercise. This is still valuable: the
  edge-case tests caught that the `SelinuxLabelMode::KeepDefault` variant
  behaves correctly in minimal mode (no `security_opt` emitted), which was not
  previously tested explicitly.
- The agent team approach worked well for this task, with unit tests and BDD
  tests developed in parallel without file conflicts.

## Context and orientation

The project is a sandboxed agent runner (`podbot`) implemented in Rust. The
container-creation logic lives in:

    src/engine/connection/create_container/mod.rs

This module defines:

- `ContainerSecurityOptions`: a struct holding `privileged`, `mount_dev_fuse`,
  and `selinux_label_mode` fields.
- `CreateContainerRequest`: the request builder, validated by `validate_image`.
- `build_host_config(security)`: translates security options into
  `bollard::models::HostConfig`.
- `EngineConnector::create_container_async`: the async entry point that builds
  the create payload and calls the engine.

Existing unit tests live in:

    src/engine/connection/create_container/tests/mod.rs
    src/engine/connection/create_container/tests/privileged_mode.rs

BDD tests live in:

    tests/bdd_container_creation.rs
    tests/bdd_container_creation_helpers/ (assertions.rs, state.rs, steps.rs)
    tests/features/container_creation.feature

The `SandboxConfig` type in `src/config/types.rs` provides the two boolean
fields (`privileged`, `mount_dev_fuse`) that feed
`ContainerSecurityOptions::from_sandbox_config`.

## Plan of work

### Stage A: Minimal-mode unit tests

Create `src/engine/connection/create_container/tests/minimal_mode.rs` following
the pattern established by `privileged_mode.rs`. The new tests validate edge
cases specific to the minimal-mode code path:

1. `from_sandbox_config_minimal_mode_sets_selinux_disable`: Verify that
   `from_sandbox_config` with `privileged = false` produces
   `SelinuxLabelMode::DisableForContainer`.

2. `minimal_mode_with_fuse_and_selinux_keep_default`: Verify that even when
   `selinux_label_mode` is explicitly set to `KeepDefault` in a non-privileged
   request, the host config respects that setting (no `security_opt`).

3. `minimal_mode_without_fuse_omits_capabilities`: Verify that when
   `mount_dev_fuse = false`, no capabilities are added (cap_add is None).

4. `minimal_mode_without_optional_fields`: Verify that a minimal request
   without name, cmd, or env produces the expected body structure.

5. `minimal_mode_with_fuse_verifies_device_details`: Verify the exact device
   mapping (path_on_host, path_in_container, cgroup_permissions) in detail.

Register the submodule in `tests/mod.rs`.

**Validation:** `make test` passes with the new tests.

### Stage B: Minimal-mode BDD scenarios

Add two new BDD scenarios to `tests/features/container_creation.feature`:

1. "Minimal mode ignores SELinux keep-default when fuse is mounted": Verify
   that when `privileged = false`, `mount_dev_fuse = true`, and
   `selinux_label_mode = KeepDefault`, the container is created successfully
   with fuse mounted but without the `label=disable` security option.

2. "Minimal mode without /dev/fuse omits capabilities": Verify that when
   `privileged = false` and `mount_dev_fuse = false`, no capabilities or device
   mappings are present (explicit edge case scenario).

Add the corresponding `given` step for the new security configuration variant,
the `when` step already exists, and add a new `then` assertion step.

Wire up the new scenarios in `bdd_container_creation.rs`.

**Validation:** `make test` passes with all scenarios.

### Stage C: Documentation verification

1. Verify `docs/podbot-design.md` accurately describes minimal mode. The
   existing "Container creation security mapping" section already documents the
   behaviour. No changes expected.

2. Verify `docs/users-guide.md` documents minimal mode clearly for end users.
   The existing "Sandbox configuration" section already describes minimal mode.
   No changes expected unless gaps are identified.

### Stage D: Quality gates and roadmap

1. Run `make check-fmt`, `make lint`, `make test`.
2. Mark the `2.2.3` entry in `docs/podbot-roadmap.md` as done (change `[ ]`
   to `[x]`).
3. Commit the changes.

## Concrete steps

All commands should be run from the repository root
`/data/leynos/Projects/podbot`.

1. Create the file
   `src/engine/connection/create_container/tests/minimal_mode.rs` with the
   edge-case tests described in Stage A.

2. Register the submodule in `tests/mod.rs` by adding `mod minimal_mode;`.

3. Run `make test` and verify the new tests pass.

4. Add new BDD scenarios to `tests/features/container_creation.feature`.

5. Add the necessary step definitions and assertions in the
   `bdd_container_creation_helpers/` directory.

6. Wire up the new scenarios in `bdd_container_creation.rs`.

7. Run `make test` and verify the new BDD scenarios pass.

8. Review `docs/podbot-design.md` and `docs/users-guide.md` for accuracy.
   Update if gaps are found.

9. Run all quality gates:

       make check-fmt
       make lint
       make test

   Expected: all three pass with zero warnings.

10. Update `docs/podbot-roadmap.md` to mark task 2.2.3 as done.

11. Commit the changes with a descriptive message.

## Validation and acceptance

Quality criteria (what "done" means):

- Tests: `make test` passes. New tests
  `tests::minimal_mode::from_sandbox_config_minimal_mode_sets_selinux_disable`,
  `tests::minimal_mode::minimal_mode_with_fuse_and_selinux_keep_default`,
  `tests::minimal_mode::minimal_mode_without_fuse_omits_capabilities`,
  `tests::minimal_mode::minimal_mode_without_optional_fields`, and
  `tests::minimal_mode::minimal_mode_with_fuse_verifies_device_details` all
  pass. New BDD scenarios pass.
- Lint/typecheck: `make check-fmt` and `make lint` pass with zero warnings.
- Documentation: design doc and user's guide accurately describe minimal mode.
- Roadmap: task 2.2.3 is marked `[x]` in `docs/podbot-roadmap.md`.

Quality method (how we check):

    make check-fmt && make lint && make test

## Idempotence and recovery

All steps are additive (new files, new test scenarios, documentation edits).
Re-running any step is safe and produces the same outcome. No destructive
operations are involved.

## Artifacts and notes

Key files that will be created or modified:

- `src/engine/connection/create_container/tests/minimal_mode.rs` (NEW)
- `src/engine/connection/create_container/tests/mod.rs` (add `mod minimal_mode`)
- `tests/features/container_creation.feature` (add 2 scenarios)
- `tests/bdd_container_creation.rs` (wire new scenarios)
- `tests/bdd_container_creation_helpers/steps.rs` (add step definitions)
- `tests/bdd_container_creation_helpers/assertions.rs` (add assertions)
- `docs/podbot-roadmap.md` (mark 2.2.3 as done)
- `docs/execplans/2-2-3-minimal-mode.md` (this file)

## Interfaces and dependencies

No new dependencies required. All work uses existing crate APIs:

- `bollard::models::HostConfig`, `DeviceMapping`
- `bollard::query_parameters::CreateContainerOptions`
- `podbot::engine::{ContainerSecurityOptions, CreateContainerRequest,
  EngineConnector, SelinuxLabelMode}`
- `podbot::config::SandboxConfig`
- `rstest::rstest`, `rstest::fixture`
- `rstest_bdd_macros::{scenario, given, when, then, ScenarioState}`
- `mockall::mock!`
