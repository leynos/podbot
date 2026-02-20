# Step 2.2.5: Set the container image from configuration

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE

No `PLANS.md` file exists in this repository as of 2026-02-20, so this ExecPlan
is the governing implementation document for this task.

## Purpose and big picture

Complete roadmap task 2.2.5 from `docs/podbot-roadmap.md`: "Set the container
image from configuration."

The core outcome is that container creation must consume the resolved
configuration image (`AppConfig.image`) rather than relying on ad-hoc test or
call-site string inputs. The source of truth remains layered configuration
resolution (defaults, file, environment, command line interface), and the
container-create request must fail semantically when the resolved image is
missing or whitespace-only.

After implementation:

- container-create requests built from configuration use the resolved `image`
  value;
- missing or whitespace-only resolved images fail with
  `ConfigError::MissingRequired { field: "image" }`;
- unit tests (with `rstest`) and behavioural tests (with `rstest-bdd` v0.5.0)
  cover happy, unhappy, and edge paths;
- `docs/podbot-design.md` records the image-selection decision;
- `docs/users-guide.md` documents any observable behaviour change;
- `docs/podbot-roadmap.md` task 2.2.5 is marked done after all gates pass.

## Constraints

- Do not change the precedence model already documented and implemented:
  defaults < file < environment variables < command-line interface.
- Keep `CreateContainerRequest::new` image validation semantics unchanged.
- Preserve existing security mapping behaviour for privileged and minimal
  modes.
- Use `rstest` fixtures and parameterized tests for unit coverage.
- Use `rstest-bdd` v0.5.0 for behavioural coverage.
- Keep module-level `//!` comments in all touched Rust modules.
- Avoid `unwrap`/`expect` in production code.
- Use en-GB-oxendict spelling in documentation updates.
- Use Make targets for required gates: `make check-fmt`, `make lint`,
  `make test`.

## Tolerances (exception triggers)

- Scope tolerance: stop and escalate if this task requires edits in more than
  14 files or more than 450 net lines.
- Interface tolerance: stop and escalate if a public API break is needed
  outside the planned additive constructor/helper changes.
- Dependency tolerance: stop and escalate before adding any new crate.
- Test tolerance: if `make lint` or `make test` still fails after two focused
  fix passes, stop and log the blocker.
- Architecture tolerance: stop and escalate if wiring configuration image into
  runtime execution requires implementing roadmap steps outside 2.2.5.

## Risks

- Risk: the current production flow (`run_agent`) is still orchestration-stub
  code, so there may be no end-to-end runtime call path yet for
  `EngineConnector::create_container_async`. Severity: medium. Likelihood:
  high. Mitigation: implement and validate a deterministic config-to-request
  seam now, and keep runtime orchestration out of scope unless required for
  2.2.5.

- Risk: behavioural tests currently treat image as scenario state rather than a
  full `AppConfig` input. Severity: medium. Likelihood: medium. Mitigation: add
  or refactor steps so at least one scenario proves image comes from
  configuration-derived state, not hard-coded request setup.

- Risk: docs already mention image configuration, so accidental wording drift
  could create contradictory statements. Severity: low. Likelihood: medium.
  Mitigation: update only the precise sections that describe container-create
  image resolution and validation timing.

## Context and orientation

Current relevant implementation points:

- `src/config/types.rs` defines `AppConfig.image` and `SandboxConfig`.
- `src/config/cli.rs` exposes `--image`.
- `src/config/loader.rs` maps `PODBOT_IMAGE` and merges layered config.
- `src/engine/connection/create_container/mod.rs` defines
  `CreateContainerRequest` and validates image presence in `new(...)`.
- `src/main.rs` currently prints resolved image in `run_agent`, but does not
  yet orchestrate container creation.

Current test surfaces:

- Unit tests under `src/engine/connection/create_container/tests/`.
- BDD container creation scenarios in
  `tests/features/container_creation.feature` and
  `tests/bdd_container_creation_helpers/`.
- Configuration precedence tests in `tests/load_config_integration.rs` and
  `tests/features/configuration.feature`.

Roadmap dependency context:

- Step 2.3 (credential injection) depends on successful container creation.

## Agent team execution model

Use a three-lane agent team during implementation to reduce cycle time while
keeping ownership clear.

Lane A (core implementation owner):

- Own config-to-create-request wiring in engine/config modules.
- Keep production semantics and errors stable.

Lane B (unit/integration tests owner):

- Own `rstest` unit coverage and config integration assertions.
- Add edge-case coverage for missing and whitespace image from resolved config.

Lane C (behaviour/docs owner):

- Own `rstest-bdd` feature and step updates.
- Own `docs/podbot-design.md`, `docs/users-guide.md`, and roadmap checkbox
  update.

Coordination rule:

- Lane A merges first; Lanes B and C rebase onto Lane A before final gate run.

## Plan of work

### Stage A: Add explicit config-to-request image mapping seam

Introduce an additive constructor/helper that builds a `CreateContainerRequest`
directly from resolved configuration. The helper must:

- source image from `AppConfig.image`;
- source security options from `AppConfig.sandbox` via
  `ContainerSecurityOptions::from_sandbox_config`;
- preserve current validation path by reusing `CreateContainerRequest::new`.

Candidate location:

- `impl CreateContainerRequest` in
  `src/engine/connection/create_container/mod.rs`.

If introducing direct `AppConfig` coupling in the engine module appears too
broad, use a narrow adapter function in a higher-level orchestration module
while preserving the same observability and testability.

### Stage B: Unit and integration test coverage with rstest

Add focused `rstest` cases for config-driven image selection.

Required unit assertions:

- Valid configured image creates request with exact image value.
- Missing configured image fails with `MissingRequired(image)`.
- Whitespace-only configured image fails with `MissingRequired(image)`.
- Security fields still pass through from `SandboxConfig` unchanged.

Required integration assertions:

- Layered config precedence reaches `AppConfig.image` correctly for file,
  environment, and command-line interface overrides.
- The config-driven request builder consumes that resolved value.

Prefer adding a dedicated test submodule if `tests/mod.rs` grows close to the
400-line guidance threshold.

### Stage C: Behavioural coverage with rstest-bdd v0.5.0

Update BDD scenarios so at least one happy path and one unhappy path prove the
container image is configuration-driven.

Expected feature additions/updates:

- Happy path: image from configuration reaches create payload.
- Unhappy path: absent/blank resolved image yields missing-image failure and no
  engine call.

Update step helpers to capture/assert the image forwarded in
`ContainerCreateBody.image` where needed, keeping scenario state and assertions
small and explicit.

### Stage D: Documentation updates

Update `docs/podbot-design.md` with the decision that image selection for
container creation is resolved from configuration layers and validated during
request construction.

Update `docs/users-guide.md` only where behaviour is user-visible, especially:

- how `--image`, `PODBOT_IMAGE`, and file config influence container creation;
- when the missing-image error is raised.

Keep wording aligned with existing guidance and avoid duplicate statements.

### Stage E: Roadmap and final validation

After all code and docs changes are complete and verified, mark task 2.2.5 done
in `docs/podbot-roadmap.md`.

Run required gates with log capture via `tee`:

    PROJECT="$(get-project)"
    BRANCH_SAFE="$(git branch --show | tr '/' '-')"
    make check-fmt 2>&1 | tee "/tmp/check-fmt-${PROJECT}-${BRANCH_SAFE}.out"
    make lint 2>&1 | tee "/tmp/lint-${PROJECT}-${BRANCH_SAFE}.out"
    make test 2>&1 | tee "/tmp/test-${PROJECT}-${BRANCH_SAFE}.out"

If any gate fails, fix and rerun until all pass.

## Validation and acceptance

Acceptance is met only when all points below are true:

- New/updated unit tests pass and demonstrate config-driven image mapping.
- New/updated BDD scenarios pass and demonstrate happy + unhappy behaviour.
- `make check-fmt` passes.
- `make lint` passes.
- `make test` passes.
- `docs/podbot-design.md` and `docs/users-guide.md` reflect final behaviour.
- `docs/podbot-roadmap.md` marks 2.2.5 as done.

## Idempotence and recovery

- All stages are additive and can be rerun safely.
- If a partial edit leaves tests failing, revert only the incomplete hunk and
  reapply stage-by-stage.
- Keep intermediate evidence logs under `/tmp` and append follow-up reruns to
  new files rather than overwriting failed-run logs.

## Progress

- [x] (2026-02-20 UTC) Gathered roadmap and dependency context for Step 2.2.5.
- [x] (2026-02-20 UTC) Reviewed design/testing guidance documents and existing
      Step 2.2 implementation/test code paths.
- [x] (2026-02-20 UTC) Drafted ExecPlan at
      `docs/execplans/2-2-5-set-container-image-from-config.md`.
- [x] (2026-02-20 UTC) Implemented Stage A by adding
      `CreateContainerRequest::from_app_config(&AppConfig)` in
      `src/engine/connection/create_container/mod.rs` and associated unit
      coverage.
- [x] (2026-02-20 UTC) Implemented Stage B with config-image resolution
      integration coverage in `tests/load_config_image_resolution.rs`.
- [x] (2026-02-20 UTC) Implemented Stage C lane work: added BDD scenario and
      assertions proving `ContainerCreateBody.image` is forwarded from
      configuration-derived state, and retained missing/blank image no-engine
      failure coverage.
- [x] (2026-02-20 UTC) Implemented Stage D lane work: updated
      `docs/podbot-design.md` and `docs/users-guide.md` with
      config-resolution/validation timing clarifications.
- [x] (2026-02-20 UTC) Completed Stage E by marking roadmap task 2.2.5 done and
      running full gates:
      `make check-fmt`, `make lint`, `make test`.

## Surprises and discoveries

- Observation: `CreateContainerRequest` already validates `image` robustly in
  `new(...)`, so 2.2.5 should reuse this path rather than duplicate validation
  logic. Impact: lower implementation risk and consistent error semantics.

- Observation: production `run_agent` orchestration remains a stub. Impact:
  this task should focus on deterministic config-to-request mapping and tests,
  without overreaching into unrelated lifecycle steps.

- Observation: existing BDD coverage classified missing-image failures and
  engine call counts, but did not capture/assert `ContainerCreateBody.image`.
  Impact: added focused capture state and assertion step to make
  configuration-derived image forwarding explicit.

- Observation: adding the new image-resolution tests pushed
  `tests/load_config_integration.rs` beyond the 400-line code-file limit.
  Impact: moved image-specific coverage into
  `tests/load_config_image_resolution.rs`.

## Decision log

- Decision: keep this document as a planning artefact first (draft phase) and
  require explicit approval before implementation. Rationale: aligns with
  execplans approval-gate discipline and prevents unapproved scope drift.
  Date/Author: 2026-02-20 / Codex.

- Decision: plan for an agent-team split (core, tests, behaviour/docs) during
  execution. Rationale: enables parallel delivery while preserving clean
  ownership and reviewability. Date/Author: 2026-02-20 / Codex.

- Decision: capture image payload directly from the mocked create request body
  in BDD helpers. Rationale: proves the final image sent to the engine equals
  the resolved configuration-derived value, not merely scenario setup state.
  Date/Author: 2026-02-20 / Codex.

- Decision: split image-resolution tests into a dedicated integration test file
  rather than suppressing lints or file-size guidance. Rationale: keeps each
  code file under the 400-line limit and retains focused test ownership.
  Date/Author: 2026-02-20 / Codex.

## Outcomes and retrospective

Implemented Step 2.2.5 end-to-end with agent-team delivery and final
integration.

- Added `CreateContainerRequest::from_app_config(&AppConfig)` and reused
  existing `CreateContainerRequest::new(...)` validation semantics.
- Added/updated unit coverage in
  `src/engine/connection/create_container/tests/mod.rs` for config-driven
  request construction.
- Added image-resolution integration coverage in
  `tests/load_config_image_resolution.rs` covering file/env/CLI precedence plus
  missing/blank unhappy paths.
- Updated BDD coverage in
  `tests/features/container_creation.feature`,
  `tests/bdd_container_creation.rs`,
  `tests/bdd_container_creation_helpers/state.rs`,
  `tests/bdd_container_creation_helpers/steps.rs`, and
  `tests/bdd_container_creation_helpers/assertions.rs` to assert image payload
  forwarding from resolved configuration and no-engine-call validation failures.
- Updated documentation in `docs/podbot-design.md` and `docs/users-guide.md` to
  clarify configuration-layer image resolution and pre-engine-call validation.
- Marked roadmap task 2.2.5 complete in `docs/podbot-roadmap.md`.
- Verification completed with passing gates: `make check-fmt`, `make lint`,
  and `make test`.
