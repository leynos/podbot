# Stabilize public library boundaries for embedding

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: IMPLEMENTED

## Purpose / big picture

After this work, an external Rust tool can depend on `podbot` as a library and
use a documented, versioned API surface without importing `clap` types, relying
on CLI-only modules, or handling opaque `eyre::Report` values. The stable
surface is intentionally small. It documents what is supported under semver
now, what remains internal, and what is still experimental until the
hosted-session, hook, validation, and MCP contracts are fully reconciled.

Observable success is not "the crate still compiles". A host-style integration
test must compile and run using only the supported public modules. Public
Rustdoc examples must compile from the perspective of an external crate.
`make check-fmt`, `make lint`, and `make test` must all pass, and the design
document, user's guide, and roadmap must agree on the supported boundary.

This is the Step 5.3 plan from `docs/podbot-roadmap.md`. The user approved
implementation on 2026-03-30, and this document now records the implemented
boundary and its evidence.

## Agent team

This plan assumes a small agent team with explicit ownership. One implementer
may execute every role, but the responsibilities should remain distinct.

1. Coordinator. Owns milestone order, tolerance checks, and approval gating.
   Keeps this document current as discoveries occur.
2. Boundary steward. Audits current `pub` items, defines the stable versus
   internal versus experimental split, and drives `lib.rs`/Cargo changes.
3. CLI boundary owner. Moves `clap` and any CLI-only helper paths behind a
   binary or feature boundary, and keeps the binary usable.
4. Contract reconciler. Compares hook and validation schemas across the ADRs,
   design docs, and Corbusier-facing integration notes, then records the chosen
   source of truth.
5. Test lead. Adds `rstest` unit coverage, `rstest-bdd` v0.5.0 behavioural
   coverage, and host-style integration tests that behave like an external
   dependent crate.
6. Documentation owner. Updates `docs/podbot-design.md`,
   `docs/users-guide.md`, the roadmap entry, and public Rustdoc examples.

## Constraints

- The dual-delivery model in `docs/podbot-design.md` remains true:
  library APIs own orchestration and semantic errors; the CLI is only an
  adapter for parsing, rendering, and process exit conversion.
- Stable public APIs must return semantic errors rooted in
  `podbot::error::PodbotError`. `eyre::Report` is allowed only at the
  application boundary (`src/main.rs`) and must not appear in public library
  signatures.
- Stable public types must not require importing `podbot::cli`,
  `podbot::engine`, or `podbot::github` (the GitHub integration module).
- CLI-only dependencies and code paths must be gated behind a binary or Cargo
  feature boundary. A library embedder must be able to avoid the CLI surface.
- The documented boundary in Architecture Decision Record (ADR) 001 is
  authoritative for direction: stable surfaces stay deliberately small;
  experimental surfaces require explicit marking and documentation.
- Hook and validation schemas must align with ADR 003, ADR 006, ADR 008, ADR
  002, ADR 007, and the Corbusier integration contract before they are
  stabilized.
- Public documentation examples must behave like external-user tests. Follow
  `docs/rust-doctest-dry-guide.md`: doctests exercise only public APIs and may
  use hidden setup or `#[cfg(doctest)]` helpers when needed.
- Unit tests use `rstest`. Behavioural tests use `rstest-bdd` v0.5.0. Follow
  existing repository rules: fixture names must match parameters, feature-file
  `{param}` captures stay unquoted, and step code uses `StepResult`-style error
  returns instead of panics or `expect`.
- Test seams must use dependency injection rather than mutating global process
  state. Follow `docs/reliable-testing-in-rust-via-dependency-injection.md` and
  existing `mockable` patterns.
- Every Rust module must begin with a `//!` comment, comments/docs use
  en-GB-oxendict spelling, and no source file may exceed 400 lines.
- Before the turn ends, run the applicable quality gates with `tee` and
  `set -o pipefail` so exit codes survive truncation. Because this step updates
  code and docs, the implementation turn must run `make check-fmt`,
  `make lint`, `make test`, and the relevant Markdown gates.

## Tolerances

- Scope: if stabilizing the boundary requires more than 25 files or roughly
  1,200 net lines of code, stop and split the work into smaller approved
  milestones.
- Packaging: if gating `clap` cleanly cannot be done with a feature/binary
  boundary and instead requires a multi-crate workspace split, stop and ask
  whether that broader packaging change is desired now.
- Compatibility: if replacing the current `podbot::api` surface would cause a
  hard breaking rename without a compatibility shim or deprecation path, stop
  and present options.
- Contract mismatch: if ADR 003/006/008 and the Corbusier-facing integration
  document disagree in a materially incompatible way, stop after documenting
  the mismatch and request a product decision on the source of truth.
- Testing: if host-style embedding tests cannot be written without exposing new
  engine-level internals, stop and re-evaluate the stable request/response
  shape before exporting more implementation details.
- Iteration budget: if the same failing gate requires more than three focused
  repair attempts, stop and document the blocker instead of thrashing.

## Risks

- Risk: the current public `podbot::api::exec` signature exposes
  `podbot::engine::{ContainerExecClient, ExecMode}` through `ExecParams`, which
  conflicts with ADR 001's goal of keeping `engine` internal. Mitigation: treat
  this as the first boundary bug to fix, not as precedent for stabilizing
  `engine`.

- Risk: `src/main.rs` currently imports
  `podbot::github::validate_app_credentials` (GitHub App validation) and
  `podbot::engine::{EngineConnector, SocketResolver, ExecMode}` directly.
  Hiding those modules without a replacement seam will break the binary.
  Mitigation: add feature-gated CLI support paths or move the needed behaviour
  fully behind stable library APIs before shrinking `lib.rs`.

- Risk: `clap` is currently an unconditional dependency in `Cargo.toml`.
  Mitigation: prefer an optional dependency plus `cli` feature and document the
  embedder dependency stanza clearly.

- Risk: the user's guide already documents `podbot::api` and currently tells
  embedders to import `podbot::engine` types. That example will become wrong as
  soon as the boundary is narrowed. Mitigation: update `docs/users-guide.md` in
  the same change, with a stable example that imports only supported modules.

- Risk: hook and validation contracts are still mostly ADR-level designs.
  Stabilizing them prematurely would create semver commitments for shapes that
  have not yet survived implementation. Mitigation: keep those surfaces
  experimental or explicitly out of scope until the contract reconciliation
  stage is complete.

## Progress

- [x] (2026-03-30 00:00 UTC) Gather roadmap, ADR, code, and testing context.
- [x] (2026-03-30 00:00 UTC) Draft this ExecPlan.
- [x] (2026-03-30 00:00 UTC) Validate the draft with `make fmt`,
  `make markdownlint`, `make nixie`, `make check-fmt`, `make lint`, and
  `make test`.
- [x] (2026-03-30) Approval gate: obtain explicit user approval for
  implementation.
- [x] (2026-03-30) Audit and classify the current public surface.
- [x] (2026-03-30) Land the feature/binary boundary for CLI-only code and
  dependencies.
- [x] (2026-03-30) Refactor stable request/response types so public APIs stop
  leaking engine internals.
- [x] (2026-03-30) Reconcile hook and validation contracts, and decide which
  remain experimental.
- [x] (2026-03-30) Add unit, behavioural, integration, and Rustdoc coverage
  for the stable boundary.
- [x] (2026-03-30) Update design docs, user's guide, and roadmap; then run
  all gates.

## Surprises & Discoveries

- `src/lib.rs` currently exports `api`, `cli`, `config`, `engine`, `error`,
  and `github` (the GitHub integration module) publicly. That is far broader
  than the intended long-term library boundary.
- `Cargo.toml` currently lists `clap` in `[dependencies]`, so library builds
  always resolve CLI parsing code even though Step 5.3 requires a boundary.
- `docs/users-guide.md` already has a "Library API" section, but the example
  imports `podbot::engine::{ContainerExecClient, ExecMode}`. That means the
  documentation currently teaches consumers to depend on an unstable layer.
- Existing orchestration BDD coverage under `tests/bdd_orchestration.rs`
  validates public functions, but it is not yet a host-style embedding test in
  the sense required by Step 5.3. It uses internal knowledge of the current API
  and does not prove the final curated surface from an external consumer's
  point of view.
- ADR 001 proposes `podbot::launch`, `podbot::session`, and `podbot::mcp` as
  the long-term stable shape, but the currently implemented and documented
  surface is `podbot::api`. Step 5.3 must reconcile that mismatch instead of
  accidentally enshrining both.
- Keeping `engine` and `github` (the GitHub integration) physically public but
  `#[doc(hidden)]` avoids detonating the existing internal test matrix in one
  step. The documented stable surface is still `api`, `config`, and `error`;
  the hidden modules are compatibility-only and explicitly unsupported for
  semver purposes.

## Decision Log

- Decision: treat the current `podbot::api` surface as the starting point for
  stabilization, not automatic proof that it is the correct final shape.
  Rationale: Step 5.1 exported `api`, but Step 5.3 is specifically responsible
  for deciding what becomes supported long-term.

- Decision: do not stabilize `podbot::engine`, `podbot::github`, or
  `podbot::cli`. Rationale: ADR 001, the design doc, and the roadmap all say
  embedders should call orchestration APIs, not low-level engine, GitHub, or
  parsing helpers.

- Decision: prefer a compatibility-preserving path over a rename-only path.
  Rationale: if `podbot::launch` is introduced now, it should be an additive
  stable alias or wrapper around the curated surface, not a breaking removal of
  `podbot::api` without a migration story.

- Decision: keep hooks, validation, session, and MCP surfaces experimental
  unless the implementation turn can prove their request/response types are
  consistent across ADRs and user-facing docs. Rationale: their contracts are
  documented but not yet fully implemented, and Step 5.3 should not create
  accidental semver promises.
- Decision: keep `podbot::api` as the stable entry point for this step rather
  than introducing `podbot::launch` prematurely. Rationale: a stable wrapper
  around the implemented orchestration surface is lower risk than promising an
  ADR-only namespace that does not exist yet.
- Decision: satisfy the CLI boundary by making `clap` optional behind the
  `cli` feature and requiring that feature for the binary target. Rationale:
  embedders can use `default-features = false`, while the operator path remains
  `cargo install --path .`.

## Outcomes & Retrospective

Implemented outcome:

- Supported stable modules are `podbot::api`, `podbot::config`, and
  `podbot::error`.
- The stable exec surface is
  `podbot::api::{ExecRequest, ExecMode, ExecContext, exec}`. It no longer
  requires engine traits, runtime handles, or CLI parse types.
- `podbot::api` remained the stable namespace for this step. No additive
  `launch` alias was introduced.
- CLI packaging now uses an optional `clap` dependency behind the `cli`
  feature, and the `podbot` binary requires that feature.
- Hook, validation, session, and MCP contracts remain experimental and are not
  part of the documented stable boundary.
- Coverage added or updated:
  - `src/api/tests.rs` unit coverage for the stable exec request and fast-fail
    validation paths.
  - `tests/bdd_orchestration.rs` behavioural coverage updated to use the
    stable exec request with a hidden compatibility seam for mocked engine IO.
  - `tests/stable_library_boundary.rs` host-style integration proof using only
    supported modules.
  - Public Rustdoc examples for `ExecRequest::new` and `exec`.
- Final gate results are recorded from the implementation run:
  `make fmt`, `MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint`,
  `make nixie`, `make check-fmt`, `make lint`, and `make test`.

## Context and orientation

The implementer should begin by reading the following files in this order:

1. `docs/podbot-roadmap.md`, especially Step 5.3 and the preceding Steps 4.8,
   4.9, and 4.10.
2. `docs/podbot-design.md`, especially the dual-delivery model and error
   handling boundary.
3. `docs/adr-001-define-the-stable-public-library-boundary.md`.
4. `docs/adr-002-define-the-hosted-session-api-and-control-channel.md`.
5. `docs/adr-003-define-the-hook-execution-primitive-and-suspend-ack-protocol.md`.
6. `docs/adr-006-define-the-validate-surface-and-capability-disposition-model.md`.
7. `docs/adr-007-define-session-composition-and-artefact-materialisation.md`.
8. `docs/adr-008-define-secrets-and-trust-boundaries-for-hooks-prompts-and-validation.md`.
9. `docs/mcp-server-hosting-design.md`, especially section 8.3.
10. `docs/users-guide.md`, especially the current "Library API" section.

Current repository state that matters:

- `src/lib.rs` exports every major module publicly.
- `src/main.rs` is a thin adapter in principle, but it still depends directly
  on `engine` and `github` (the GitHub integration) public modules.
- `src/api/mod.rs` and `src/api/exec.rs` provide the current orchestration
  surface.
- `src/error.rs` already defines `PodbotError` correctly, so this step should
  preserve that pattern rather than invent a new error boundary.
- `src/cli/mod.rs` carries `clap`-derived parse types and is currently public.
- `tests/bdd_orchestration.rs` covers the current orchestration API; this is a
  useful starting point but not the finished embedding proof.

The key technical tension is that the current stable-looking surface leaks
engine implementation details. A genuine stable boundary must let embedders use
Podbot without importing container-engine plumbing or CLI parse types.

## Plan of work

### Stage A: Build the boundary inventory and write the target classification

Start with a classification document in the implementation branch notes or in
the updated design document. Enumerate every public module and public type that
is reachable from `src/lib.rs` or from currently documented examples. Group
them into four buckets:

1. stable now,
2. stable later but not yet implemented,
3. experimental preview, and
4. internal only.

The expected starting classification is:

- Stable now candidate: `podbot::api`, `podbot::config`, `podbot::error`.
- Stable later candidate: `podbot::launch`, `podbot::session`, `podbot::mcp`
  once their request/response types exist and are tested.
- Experimental preview candidate: `podbot::hooks`, `podbot::validate`,
  possibly `podbot::session` and `podbot::mcp` if implemented only partially.
- Internal only: `podbot::cli`, `podbot::engine`, `podbot::github` (the
  GitHub integration module).

Do not skip this audit. The implementation turn should begin by writing one or
more red tests that describe the intended stable import paths and the modules
that must no longer be supported by external consumers.

### Stage B: Establish the binary or feature boundary for CLI-only code

Change `Cargo.toml` and `src/lib.rs` so CLI-only parsing code is gated behind a
Cargo feature or binary-only path. The intended result is:

- `clap` is no longer an unconditional library dependency.
- `podbot::cli` is not part of the default stable library boundary.
- the `podbot` binary still builds and runs in normal operator workflows.

The preferred implementation is an optional `clap` dependency plus a `cli`
feature, with the binary target requiring that feature. If that proves too
disruptive, stop at the tolerance gate and ask whether a package split is
acceptable.

This stage must also deal with any helper code the binary currently imports
from `engine` and `github`. The binary may keep using feature-gated support
paths, but those paths must not be documented as stable for embedders.

### Stage C: Refactor public request/response types to remove engine leakage

This is the heart of the stabilization work. The current `ExecParams` requires
`ContainerExecClient` and `ExecMode` from `podbot::engine`. That means the
public API is not yet truly library-friendly.

Refactor the supported orchestration entry points so their documented public
signatures reference only stable request/response types from stable modules.
Possible acceptable outcomes include:

1. New stable request types inside `podbot::api` or `podbot::launch`.
2. An additive stable wrapper over the current low-level orchestration seam.
3. A feature-gated internal seam kept for tests while the stable surface uses
   only library-owned types.

The wrong outcome is exporting more of `podbot::engine` just because the
current tests use it.

At the end of this stage, public Rustdoc and integration tests should no longer
need `podbot::engine` imports to call the supported library API.

### Stage D: Reconcile hook and validation schemas before promising stability

Compare the hook and validation shapes in:

- `docs/adr-003-define-the-hook-execution-primitive-and-suspend-ack-protocol.md`,
- `docs/adr-006-define-the-validate-surface-and-capability-disposition-model.md`,
- `docs/adr-008-define-secrets-and-trust-boundaries-for-hooks-prompts-and-validation.md`,
- `docs/adr-002-define-the-hosted-session-api-and-control-channel.md`,
- `docs/adr-007-define-session-composition-and-artefact-materialisation.md`, and
- `docs/corbusier-conformance-design-for-agents-mcp-wires-and-hooks.md`.

Record every mismatch. The most obvious one today is validation taxonomy: ADR
006 uses `Native`, `HostEnforced`, `Translated`, `Ignored`, and `Invalid`,
while the Corbusier integration note still shows `Supported`, `Ignored`,
`Rejected`, and `Unknown`.

The implementation turn must choose one of two outcomes and document it
explicitly:

1. The contract is reconciled and exported as an experimental preview with the
   agreed request/response types.
2. The contract is not yet stable enough, so it remains documented but
   unexported from the supported default surface.

Do not stabilize hook or validation types until this stage has a clear written
answer.

### Stage E: Add host-style tests that exercise the boundary from outside

Add tests in three layers.

First, add `rstest`-based unit tests for any new stable request/response types,
feature gating logic, and `PodbotError`-based unhappy paths.

Second, add `rstest-bdd` v0.5.0 behavioural scenarios that describe the host
embedder experience. Suggested scenarios:

- importing and calling the supported orchestration entry point without any CLI
  parse types,
- receiving `PodbotError` rather than `eyre::Report`,
- verifying that unsupported CLI-only modules are not part of the documented
  stable contract, and
- verifying the chosen experimental-gating behaviour for hook or validation
  surfaces.

Third, add true host-style integration proof. This should compile and execute
as an external consumer would. Use either a dedicated integration test crate
under `tests/` or public Rustdoc examples with shared hidden helpers. The test
must import only supported modules and must not rely on crate-private seams.

If the stable surface becomes feature-sensitive, add one integration test that
proves the embedder path with the CLI feature disabled.

### Stage F: Update the design document, user's guide, and roadmap together

Update `docs/podbot-design.md` to record the stable boundary and any decisions
taken during implementation. Update ADR 001 if the implemented boundary differs
materially from the current proposal, or add a superseding decision note in the
design document if ADR changes are intentionally deferred.

Update `docs/users-guide.md` only for user-visible behaviour and supported
embedding guidance. Replace the current library example so it uses only stable
types. If the CLI feature changes installation or build commands, document the
new operator path clearly.

Finally, mark Step 5.3 as done in `docs/podbot-roadmap.md` only after the
tests, docs, and boundary changes are all complete.

## Validation and evidence

During implementation, capture evidence in this order.

1. Run formatting before Markdown linting because Markdown tools can rewrite
   files:

```bash
set -o pipefail && make fmt 2>&1 | tee /tmp/podbot-fmt.log
```

1. Run Markdown validation for the updated docs:

```bash
set -o pipefail && MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint 2>&1 | tee /tmp/podbot-markdownlint.log
set -o pipefail && make nixie 2>&1 | tee /tmp/podbot-nixie.log
```

1. Run the required Rust quality gates:

```bash
set -o pipefail && make check-fmt 2>&1 | tee /tmp/podbot-check-fmt.log
set -o pipefail && make lint 2>&1 | tee /tmp/podbot-lint.log
set -o pipefail && make test 2>&1 | tee /tmp/podbot-test.log
```

Expected evidence for completion:

- the stable import examples compile from outside the crate,
- no public library signature returns `eyre`,
- the binary still builds through the intended CLI path,
- the user's guide matches the implemented embedding contract, and
- Step 5.3 is marked complete in the roadmap.

## Approval checkpoint

Implementation must not start until the user explicitly approves this plan or
requests revisions. The first implementation action after approval should be to
add the red tests for the intended stable boundary, then proceed stage by stage.
