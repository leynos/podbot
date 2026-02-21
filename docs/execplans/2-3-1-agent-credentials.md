# Step 2.3.1: Inject agent credentials into the container filesystem

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE (2026-02-21 UTC)

No `PLANS.md` file exists in this repository as of 2026-02-20, so this ExecPlan
is the governing implementation document for this task.

Implementation completed on branch `2-3-1-agent-credentials`; this plan now
records delivery and validation evidence.

## Purpose and big picture

Complete roadmap Step 2.3 ("Credential injection") by adding a Bollard-backed
upload path that copies host agent credentials into the container filesystem
without bind-mounting the host home directory.

After this change, the container lifecycle will support a deterministic
credential injection step that:

- uploads `~/.claude` when `creds.copy_claude` is enabled;
- uploads `~/.codex` when `creds.copy_codex` is enabled;
- preserves file permissions in the uploaded archive;
- verifies that expected credential paths are present in the container view.

The behaviour must be observable through focused unit tests (`rstest`) and
behavioural tests (`rstest-bdd` v0.5.0), including happy and unhappy paths.

Required documentation outcomes for this task:

- record design decisions in `docs/podbot-design.md`;
- update user-visible behaviour in `docs/users-guide.md`;
- mark Step 2.3 roadmap tasks as done in `docs/podbot-roadmap.md` after tests
  and gates pass.

## Constraints

- Keep existing engine connection and container creation behaviour unchanged.
- Do not regress Step 2.1 and Step 2.2 tests.
- Preserve existing configuration semantics in `AppConfig` and `CredsConfig`.
- Avoid `unwrap` and `expect` outside test code.
- Keep module-level `//!` docs in any Rust module added or touched.
- Use capability-oriented filesystem access (`cap_std::fs_utf8` and `camino`)
  in new filesystem-facing production code.
- Use `rstest` fixtures for unit tests and `rstest-bdd` v0.5.0 for behavioural
  tests.
- Keep files under 400 lines where practical; split modules if needed.
- Use en-GB-oxendict spelling in documentation updates.
- Validate with `make check-fmt`, `make lint`, and `make test`.
- Because docs are updated, also run `make markdownlint`, `make fmt`, and
  `make nixie` before finalizing.
- Commit each logical change as an atomic commit, and gate each commit.

## Tolerances (exception triggers)

- Scope: if implementation requires changes in more than 14 files or more than
  550 net lines, stop and confirm scope.
- API: if existing public API signatures must change in a breaking way, stop
  and confirm migration strategy.
- Dependencies: if a new crate is needed for tar archive authoring, stop and
  confirm before adding it.
- Semantics: if expected in-container credential target paths are ambiguous,
  stop and confirm before shipping.
- Iterations: if `make lint` or `make test` still fails after two fix passes,
  stop and record blocker evidence.
- Runtime: if behavioural verification cannot be implemented without Step 2.4
  interactive exec work, stop and agree on an acceptable verification seam.

## Risks

- Risk: `upload_to_container` needs a tar stream and permission metadata.
  Severity: medium. Likelihood: medium. Mitigation: isolate tar construction in
  a small helper API with dedicated unit tests that inspect archive headers.
- Risk: one credential directory may be missing on a host while copy toggles
  default to `true`. Severity: medium. Likelihood: high. Mitigation: define and
  test explicit missing-source behaviour for each configured credential source.
- Risk: there is no implemented end-to-end container run path in `main.rs` yet.
  Severity: medium. Likelihood: high. Mitigation: verify via deterministic
  uploader abstractions and behavioural seams that assert upload requests and
  extracted tar paths.
- Risk: code can become complex if config selection, filesystem walking, tar
  building, and engine calls are mixed in one function. Severity: medium.
  Likelihood: medium. Mitigation: split responsibilities into small functions
  and helper structs.

## Progress

- [x] (2026-02-20 UTC) Reviewed roadmap, design, user guide, and testing guides
      relevant to Step 2.3.
- [x] (2026-02-20 UTC) Inspected current engine module and confirmed no existing
      `upload_to_container` implementation.
- [x] (2026-02-20 UTC) Drafted this ExecPlan.
- [x] (2026-02-21 UTC) Implemented credential upload module and tar builder in
      `src/engine/connection/upload_credentials/`.
- [x] (2026-02-21 UTC) Added unit tests for happy, unhappy, and edge-path
      credential upload behaviour.
- [x] (2026-02-21 UTC) Added behavioural scenarios in
      `tests/features/credential_injection.feature` and related helpers.
- [x] (2026-02-21 UTC) Updated `docs/podbot-design.md` and
      `docs/users-guide.md` with the final credential injection contract and
      user-visible behaviour.
- [x] (2026-02-21 UTC) Marked Step 2.3 roadmap tasks as done in
      `docs/podbot-roadmap.md`.
- [x] (2026-02-21 UTC) Ran docs gates and captured logs:
      `/tmp/markdownlint-podbot-2-3-1-agent-credentials.out`,
      `/tmp/fmt-podbot-2-3-1-agent-credentials.out`,
      `/tmp/nixie-podbot-2-3-1-agent-credentials.out`.
- [x] (2026-02-21 UTC) Finalized outcomes and retrospective section.

## Surprises and discoveries

- Observation: `ContainerError::UploadFailed` already exists in `src/error.rs`,
  so this task can map upload failures without introducing a new top-level
  error variant.
- Observation: `CredsConfig` (`copy_claude`, `copy_codex`) is fully wired into
  config defaults and environment loading, but no engine code consumes it yet.
- Observation: `src/main.rs` remains orchestration-stubbed, so this step should
  be implemented through reusable engine-level APIs and test seams first.

## Decision log

- Decision: initial plan drafting was kept as plan-only work until
  implementation moved forward in the branch. Rationale: preserve explicit
  approval boundaries for plan-first workflow. Date/Author: 2026-02-20 / Codex.
- Decision: implementation will use an agent team with explicit file ownership
  to keep changes parallelizable and avoid merge churn. Date/Author: 2026-02-20
  / Codex.
- Decision: canonical in-container credential targets are `/root/.claude` and
  `/root/.codex`, uploaded via Bollard `upload_to_container` with
  `path = "/root"`. Rationale: align with root-home conventions in the sandbox
  image and keep target paths deterministic. Date/Author: 2026-02-21 / Codex.
- Decision: selected credential sources that are missing on the host are
  skipped; if no selected sources are present, upload returns success without
  sending an upload request. Rationale: support hosts configured for one agent
  while keeping defaults enabled for both toggles. Date/Author: 2026-02-21 /
  Codex.
- Decision: upload-path errors (tar build or daemon upload) map to
  `ContainerError::UploadFailed`. Rationale: preserve semantic error handling
  with container-context diagnostics. Date/Author: 2026-02-21 / Codex.
- Decision: tar entries preserve source permission metadata for files and
  directories. Rationale: avoid credential-read regressions caused by
  permission drift. Date/Author: 2026-02-21 / Codex.

## Outcomes and retrospective

Step 2.3.1 implementation outcomes:

- Engine credential injection now uploads selected `~/.claude` and `~/.codex`
  directories into `/root` via Bollard tar upload.
- Missing selected source directories are skipped, and all-empty selection
  resolves as a no-op success.
- Upload results report expected in-container paths for selected and present
  credential families.
- Tar archive entries preserve source permission metadata.
- Upload failures are mapped to `ContainerError::UploadFailed`.

Verification outcomes recorded in this docs-owner slice:

- Documentation updates for design contract, user guidance, and roadmap status
  are complete.
- Docs gates passed: `make markdownlint`, `make fmt`, and `make nixie`.

Retrospective notes:

- The uploader abstraction and archive helper split kept behavioural and unit
  coverage deterministic without daemon dependencies.
- Formalizing skip/no-op semantics removed ambiguity for mixed-agent host
  setups and made user-facing expectations clearer.

## Context and orientation

Current relevant files:

- `src/engine/connection/mod.rs` exports engine connection and container
  creation surfaces, but no upload API.
- `src/engine/connection/create_container/mod.rs` contains existing lifecycle
  request modelling patterns (`ContainerCreator`, request structs, sync/async
  wrappers) that should be mirrored for upload.
- `src/config/types.rs` defines `CredsConfig` and `AppConfig.creds`.
- `src/config/loader.rs` maps `PODBOT_CREDS_COPY_CLAUDE` and
  `PODBOT_CREDS_COPY_CODEX`.
- `tests/bdd_container_creation*` and
  `tests/bdd_container_creation_helpers/*` show the current behavioural test
  seam pattern with `rstest-bdd`.
- `docs/podbot-roadmap.md` defines Step 2.3 tasks and completion criteria.
- `docs/podbot-design.md` already specifies that credentials must be copied via
  Bollard `upload_to_container` tar uploads.
- `docs/users-guide.md` already documents credential toggles but does not yet
  describe concrete upload behaviour or verification expectations.

## Agent team and ownership

Implementation will use a three-agent team plus integrator:

- Agent A (engine upload core): owns `src/engine/connection/*` credential upload
  module, uploader trait abstraction, tar archive construction, and error
  mapping.
- Agent B (unit tests): owns unit tests for tar contents, path mapping, toggle
  handling, and unhappy paths around missing dirs and upload failures.
- Agent C (behaviour + docs): owns `rstest-bdd` scenarios and step helpers plus
  docs updates (`docs/podbot-design.md`, `docs/users-guide.md`,
  `docs/podbot-roadmap.md`).
- Integrator (lead agent): resolves conflicts, runs gates, keeps this ExecPlan
  updated, and performs final commits.

All agents must treat concurrent changes outside their owned files as valid and
must not revert other agents' edits.

## Plan of work

### Stage A: Finalize credential upload contract

Define contract types and semantics before coding:

- input: resolved `AppConfig.creds`, host home directory, and target container
  identifier;
- output: deterministic set of upload operations (0, 1, or 2);
- error mapping: engine upload failures map to `ContainerError::UploadFailed`
  with the target container ID.

Lock missing-source semantics per credential directory (skip vs fail) and
document rationale.

Validation gate:

- decision captured in `Decision log`;
- target container paths are explicit and unambiguous.

### Stage B: Implement tar archive builder with permission preservation

Add a dedicated helper that:

- enumerates files under selected source dirs (`~/.claude`, `~/.codex`);
- writes tar entries for each file/dir with deterministic target prefixes;
- preserves mode bits and directory/file distinctions in tar headers.

Implementation guidance:

- prefer capability-oriented access (`cap_std::fs_utf8`) and `camino` paths;
- keep tar-building and upload transport separate for testability.

Validation gate:

- unit tests inspect generated tar entries and verify modes and paths.

### Stage C: Implement Bollard upload wrapper

Add an upload abstraction similar to `ContainerCreator`, for example:

- `ContainerUploader` trait returning boxed future;
- `Docker` implementation delegating to `upload_to_container`;
- `EngineConnector` async/sync helpers for credential upload.

Behaviour:

- upload selected archive payload into the agreed in-container base path;
- when both credential families are enabled, upload both in deterministic order;
- when a family is disabled, do not include it in archive payload.

Validation gate:

- unit tests assert uploader invocation count, path/options, and mapped errors.

### Stage D: Add rstest unit coverage

Add focused unit tests for happy, unhappy, and edge cases:

- happy: both credential families enabled and present;
- happy: only Claude enabled;
- happy: only Codex enabled;
- unhappy: uploader returns error and maps to `UploadFailed`;
- edge: missing source directory handling follows chosen contract;
- edge: permission metadata in tar matches source file modes;
- edge: no upload attempted when both toggles disabled.

Use deterministic fixtures and test doubles; avoid daemon dependencies.

Validation gate:

- targeted unit tests pass before broader test runs.

### Stage E: Add rstest-bdd behavioural scenarios

Create a credential-injection feature suite, likely under:

- `tests/features/credential_injection.feature`;
- `tests/bdd_credential_injection.rs`;
- `tests/bdd_credential_injection_helpers/`.

Proposed scenarios:

- copy both credential directories when both toggles enabled;
- copy only `.claude` when `.codex` copy is disabled;
- copy only `.codex` when `.claude` copy is disabled;
- surface upload errors with actionable error classification;
- verify expected in-container credential paths are present.

Validation gate:

- behavioural scenarios pass under `make test`;
- scenarios cover at least one unhappy path and one edge case.

### Stage F: Documentation updates

Update docs after behaviour is implemented:

- `docs/podbot-design.md`: capture final upload contract, target paths, and
  permission-preservation design decision.
- `docs/users-guide.md`: document credential copy behaviour, toggle effects,
  and expected in-container location semantics.
- `docs/podbot-roadmap.md`: mark Step 2.3 task checklist as done only after all
  tests and required gates pass.

Validation gate:

- docs accurately match code behaviour and test assertions.

### Stage G: Quality gates and commits

Commit in small logical units, gating each commit. Recommended slices:

1. core upload + tar builder + unit tests;
2. behavioural tests;
3. docs + roadmap updates.

Required commands (capture logs with `tee`):

    set -o pipefail
    make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-2-3-1-agent-credentials.out

    set -o pipefail
    make lint 2>&1 | tee /tmp/lint-podbot-2-3-1-agent-credentials.out

    set -o pipefail
    make test 2>&1 | tee /tmp/test-podbot-2-3-1-agent-credentials.out

For docs changes:

    set -o pipefail
    make markdownlint 2>&1 | tee /tmp/markdownlint-podbot-2-3-1-agent-credentials.out

    set -o pipefail
    make fmt 2>&1 | tee /tmp/fmt-podbot-2-3-1-agent-credentials.out

    set -o pipefail
    make nixie 2>&1 | tee /tmp/nixie-podbot-2-3-1-agent-credentials.out

## Concrete command checklist

All commands run from `/data/leynos/Projects/podbot` on branch
`2-3-1-agent-credentials`.

1. Confirm branch and clean baseline status.
2. Implement Stage A-C code changes.
3. Run targeted unit tests for the new upload module.
4. Implement Stage E behavioural tests.
5. Run full `make test`.
6. Update design and user docs.
7. Mark Step 2.3 roadmap tasks as done.
8. Run full quality gates (`check-fmt`, `lint`, `test`) and docs gates.
9. Commit each logical slice after corresponding gates pass.
10. Update `Progress`, `Decision log`, and `Outcomes and retrospective`.

## Validation and acceptance criteria

Implementation is complete only when all are true:

- Roadmap Step 2.3 tasks are all checked in `docs/podbot-roadmap.md`.
- A production upload path uses Bollard `upload_to_container` with tar payloads.
- `~/.claude` and `~/.codex` upload behaviour follows `CredsConfig` toggles.
- Tar uploads preserve file permissions needed for agent credential reads.
- Tests verify expected in-container credential paths.
- Unit tests (`rstest`) include happy, unhappy, and edge cases.
- Behavioural tests (`rstest-bdd` v0.5.0) cover observable credential injection
  behaviour and at least one failure path.
- `docs/podbot-design.md` records design decisions taken.
- `docs/users-guide.md` documents user-visible behaviour changes.
- `make check-fmt`, `make lint`, and `make test` all pass.
- Docs validation gates (`make markdownlint`, `make fmt`, `make nixie`) pass.
