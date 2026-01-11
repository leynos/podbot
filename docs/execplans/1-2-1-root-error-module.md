# Step 1.2: Root error module foundation

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

PLANS.md is not present in the repository root, so this plan stands alone.

## Purpose / Big Picture

The podbot codebase needs a consistent error handling foundation so every
module can return semantic errors and the CLI can report failures cleanly at
the application boundary. The outcome is a single root error module that
defines semantic error enums, a top-level application error type, and a shared
Result alias, with unit tests (rstest) and behavioural tests (rstest-bdd)
proving both happy and unhappy paths. Success is observable by running the test
suite and seeing the new error tests pass while existing behaviour remains
unchanged.

## Constraints

- Keep semantic errors in `thiserror` enums and keep opaque errors (`eyre`) only
  at the application boundary (`src/main.rs`).
- Do not introduce new external dependencies unless strictly necessary; if a
  new crate is required, stop and escalate.
- Do not use `unwrap` or `expect` outside tests; search and remediate if found.
- All Rust modules must keep their module-level `//!` doc comment.
- Keep files under 400 lines; split as needed rather than expanding a single
  module beyond the limit.
- Use en-GB spelling in comments and documentation edits.
- Any documentation edits must follow `docs/documentation-style-guide.md` and
  wrap paragraphs at 80 columns.

## Tolerances (Exception Triggers)

- Scope: if the change requires edits to more than 8 files or more than 300 net
  lines of code, stop and escalate.
- Interface: if any public API outside `podbot::error` must change, stop and
  escalate.
- Dependencies: if a new dependency is required, stop and escalate.
- Tests: if `make lint` or `make test` fails twice after fixes, stop and
  escalate with the failure logs.
- Ambiguity: if multiple valid error module designs remain after reviewing
  `docs/podbot-design.md`, stop and present options with trade-offs.

## Risks

    - Risk: Existing error handling already satisfies the roadmap, so changes
      could be redundant or cause unnecessary churn.
      Severity: low
      Likelihood: medium
      Mitigation: Audit current `src/error.rs` and only change what is required
      for the roadmap criteria and tests.

    - Risk: Behavioural tests might struggle to express error handling without
      a running CLI path.
      Severity: medium
      Likelihood: medium
      Mitigation: Treat error display as behaviour by driving error values
      through step definitions and asserting user-visible messages.

## Progress

    - [x] (2026-01-11 01:05Z) Reviewed current error handling files and roadmap
      deltas.
    - [x] (2026-01-11 01:15Z) Refined the root error module with additional
      unit coverage.
    - [x] (2026-01-11 01:20Z) Added rstest unit coverage for error types and
      edge cases.
    - [x] (2026-01-11 01:25Z) Added rstest-bdd behavioural coverage for error
      reporting behaviour.
    - [x] (2026-01-11 01:30Z) Updated design docs and marked the roadmap entry
      done.
    - [x] (2026-01-11 01:45Z) Ran `make check-fmt`, `make lint`, and
      `make test` with logs.

## Surprises & Discoveries

    - Observation: Qdrant notes lookup failed with "Unexpected response type".
      Evidence: `qdrant-find` returned a tool call error.
      Impact: Proceeded without project memory.

    - Observation: `make markdownlint` failed because `markdownlint-cli2` was
      not on PATH.
      Evidence: Makefile error 127 until `MDLINT` was set explicitly.
      Impact: Validation used `MDLINT=/root/.bun/bin/markdownlint-cli2`.

## Decision Log

    - Decision: Keep `src/error.rs` as the root error module and extend tests
      instead of refactoring error types.
      Rationale: The existing design already matches the roadmap; tests and
      docs were the remaining gaps.
      Date/Author: 2026-01-11 / Codex

    - Decision: Store `Arc<PodbotError>` in BDD state fixtures.
      Rationale: `rstest_bdd::Slot::get` requires `Clone`; `Arc` preserves
      shared ownership without changing the error types.
      Date/Author: 2026-01-11 / Codex

## Outcomes & Retrospective

    - Outcome: Added unit and behavioural coverage for error handling, updated
      design documentation, and marked the roadmap task complete.
    - Outcome: `make check-fmt`, `make lint`, and `make test` all succeed with
      logs captured.
    - Lesson: When BDD state needs to store non-`Clone` errors, wrap them in
      `Arc` to satisfy `Slot` requirements.

## Context and Orientation

The current repository already contains `src/error.rs`, `src/main.rs`, and
`src/lib.rs`. The error module defines domain error enums and a `PodbotError`
wrapper, while `main` returns `eyre::Result<()>`. The roadmap still lists Step
1.2 as incomplete, so this plan focuses on verifying the existing module,
aligning it with the intended pattern, and adding behavioural tests and
documentation updates required by the roadmap. Relevant references:

- `docs/podbot-roadmap.md` Step 1.2
- `docs/podbot-design.md` (design decisions)
- `docs/rust-testing-with-rstest-fixtures.md` (unit test patterns)
- `docs/rstest-bdd-users-guide.md` (behavioural tests)
- `docs/reliable-testing-in-rust-via-dependency-injection.md`
- `docs/complexity-antipatterns-and-refactoring-strategies.md`
- `docs/ortho-config-users-guide.md`
- `docs/users-guide.md`

## Plan of Work

Stage A: inspect and confirm requirements. Review `src/error.rs`,
`src/main.rs`, `src/lib.rs`, and related modules to confirm the existing error
handling pattern matches the roadmap. Use ripgrep to confirm there are no
`unwrap` or `expect` calls outside test code. Capture any gaps (missing error
variants, missing Result alias, missing module exports).

Stage B: implement or refine the root error module. If gaps exist, adjust
`src/error.rs` to define the canonical semantic error enums, the top-level
`PodbotError`, and a `Result<T>` alias. Ensure conversion paths (`#[from]`) are
explicit and the module-level docs explain the boundary with `eyre::Report`.
Update other modules to use `podbot::error::Result` where appropriate, without
changing public APIs outside the error module.

Stage C: tests. Add unit tests in `src/error.rs` (or a dedicated test module)
using `rstest` to cover happy and unhappy paths plus edge cases such as missing
fields, invalid values, and error wrapping. Add rstest-bdd behavioural tests in
`tests/bdd_error_handling.rs` with a feature file under
`tests/features/error_handling.feature`, asserting user-visible messages
derived from error displays. Ensure behaviour tests cover at least one success
case (e.g., mapping to a friendly message) and one failure case (e.g., invalid
configuration message).

Stage D: documentation and roadmap. Record any design decisions in
`docs/podbot-design.md` (for example, the canonical error hierarchy and how it
maps to `eyre`). Update `docs/users-guide.md` only if the user-visible error
behaviour or messaging changes. Mark Step 1.2 in `docs/podbot-roadmap.md` as
"done" once all validations pass.

## Concrete Steps

1. Review the existing error module and confirm alignment with the roadmap.

    rg --hidden --line-number "unwrap\(|expect\(" src tests

2. Adjust error module code if needed, keeping public API changes limited to
   `podbot::error`.

3. Add/extend unit tests using `rstest` in `src/error.rs` or a new test module.

4. Add behavioural tests using `rstest-bdd`:

    - Create `tests/features/error_handling.feature` with scenarios for
      happy/unhappy paths.
    - Create `tests/bdd_error_handling.rs` with fixtures and step definitions.

5. Update `docs/podbot-design.md` with any new error-handling decisions.

6. Update `docs/users-guide.md` only if user-facing error behaviour changes.

7. Mark Step 1.2 as done in `docs/podbot-roadmap.md`.

8. Run formatting and validation commands with logs (use tee + pipefail):

    set -o pipefail
    make check-fmt | tee /tmp/podbot-check-fmt.log
    make lint | tee /tmp/podbot-lint.log
    make test | tee /tmp/podbot-test.log

   If documentation changed, also run:

    make fmt | tee /tmp/podbot-fmt.log
    make markdownlint | tee /tmp/podbot-markdownlint.log
    make nixie | tee /tmp/podbot-nixie.log

## Validation and Acceptance

Behavioural acceptance:

- The new rstest-bdd scenarios in `tests/features/error_handling.feature`
  execute via `cargo test` and assert user-visible error messages for at least
  one happy path and one unhappy path.
- The new rstest unit tests validate error display formatting and wrapping.

Quality criteria:

- Tests: `make test` passes with the new unit and behavioural tests.
- Lint/typecheck: `make lint` passes with no Clippy warnings.
- Formatting: `make check-fmt` passes; if docs changed, `make fmt` and
  `make markdownlint`/`make nixie` pass.
- Roadmap: Step 1.2 is marked done in `docs/podbot-roadmap.md`.

Quality method (how we check):

- Run the commands listed in Concrete Steps and confirm success from their
  logs.

## Idempotence and Recovery

All steps are additive or edits to existing files and can be re-run safely. If
any command fails, fix the reported issue and re-run the failed command with
the same log file name, overwriting the log to keep the latest output.

## Artifacts and Notes

Expected successful command signals (examples):

    make lint
    Finished `dev` profile [unoptimized + debuginfo] target(s) in <time>

    make test
    test result: ok. <N> passed; 0 failed

## Interfaces and Dependencies

- Root error module: `src/error.rs` with `ConfigError`, `ContainerError`,
  `GitHubError`, `FilesystemError`, `PodbotError`, and `Result<T>`.
- Public export: `src/lib.rs` should continue to expose `pub mod error`.
- Application boundary: `src/main.rs` returns `eyre::Result<()>` and converts
  domain errors to reports only at the boundary.
- Tests: `tests/bdd_error_handling.rs` and
  `tests/features/error_handling.feature` for behavioural coverage.
- No new dependencies; reuse `thiserror`, `eyre`, `rstest`, and `rstest-bdd`.

## Revision note

- Updated status, progress, decisions, and outcomes after implementing the
  error handling tests and documentation changes.
- Noted tooling surprises around Qdrant notes and markdownlint PATH.
- No remaining work; the plan is complete.
