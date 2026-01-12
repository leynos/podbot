# Step 1.2: Eyre report boundary

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: DRAFT

PLANS.md is not present in the repository root, so this plan stands alone.

## Purpose / Big Picture

The podbot CLI needs a clear error boundary: domain modules return semantic
errors, and the application entry point returns `eyre::Report` for human-
readable diagnostics. The outcome is observable by inspecting `src/main.rs`,
verifying no `unwrap` or `expect` calls remain outside test code, and running
unit and behavioural tests that cover success, failure, and edge cases for the
error boundary. Success means `make all` passes without warnings.

## Constraints

- Keep semantic errors in `thiserror` enums and keep opaque errors (`eyre`) only
  at the application boundary (`src/main.rs`).
- Do not introduce new external dependencies; if a new crate is required, stop
  and escalate.
- Do not use `unwrap` or `expect` outside tests; search and remediate if found.
- All Rust modules must keep their module-level `//!` doc comment.
- Keep files under 400 lines; split modules instead of expanding beyond this
  limit.
- Use en-GB spelling in comments and documentation edits.
- Documentation edits must follow `docs/documentation-style-guide.md` and wrap
  paragraphs at 80 columns.

## Tolerances (Exception Triggers)

- Scope: if implementation requires edits to more than 6 files or more than
  220 net lines of code, stop and escalate.
- Interface: if any public API outside `podbot::error` must change, stop and
  escalate.
- Dependencies: if a new dependency is required, stop and escalate.
- Tests: if `make all` fails twice after fixes, stop and escalate with logs.
- Ambiguity: if multiple valid approaches to the error boundary remain after
  reviewing `docs/podbot-design.md`, stop and present options with trade-offs.

## Risks

    - Risk: The entry point already returns `eyre::Result`, so changes might be
      minimal and tests could feel redundant.
      Severity: low
      Likelihood: medium
      Mitigation: Verify current behaviour, then scope tests to the exact
      boundary behaviour (converting `PodbotError` to `eyre::Report`).

    - Risk: Behavioural tests may struggle to model `eyre` output without
      invoking the CLI.
      Severity: medium
      Likelihood: medium
      Mitigation: Treat `eyre::Report` formatting as the observable behaviour
      and validate it through rstest-bdd fixtures and steps.

## Progress

    - [ ] (YYYY-MM-DD HH:MMZ) Review current error boundary and existing tests.
    - [ ] Implement boundary adjustments and remove non-test unwrap/expect.
    - [ ] Add rstest unit coverage for report conversion paths.
    - [ ] Add rstest-bdd scenarios for report output.
    - [ ] Update design and user documentation as needed.
    - [ ] Mark roadmap step complete and run validation commands.

## Surprises & Discoveries

    - Observation:
      Evidence:
      Impact:

## Decision Log

    - Decision:
      Rationale:
      Date/Author:

## Outcomes & Retrospective

    - Outcome:
    - Lesson:

## Context and Orientation

Relevant code lives in `src/main.rs` (CLI entry point) and `src/error.rs`
(semantic errors and `PodbotError`). Behavioural tests for error messaging
already exist under `tests/bdd_error_handling.rs` and
`tests/features/error_handling.feature`. Reference documents:

- `docs/podbot-roadmap.md` (Step 1.2 tasks and completion criteria)
- `docs/podbot-design.md` (design decisions for error boundaries)
- `docs/rust-testing-with-rstest-fixtures.md` (unit test patterns)
- `docs/rstest-bdd-users-guide.md` (behavioural tests)
- `docs/rust-doctest-dry-guide.md` (doctest guidance, if new doc examples are
  added)
- `docs/reliable-testing-in-rust-via-dependency-injection.md`
- `docs/complexity-antipatterns-and-refactoring-strategies.md`
- `docs/ortho-config-users-guide.md`
- `docs/users-guide.md`

## Plan of Work

Stage A: inspect and confirm requirements. Review `src/main.rs` to confirm the
entry point returns `eyre::Result<()>` and that domain errors do not leak past
the boundary. Use ripgrep to ensure there are no `unwrap` or `expect` calls
outside test modules. Review existing error handling tests to see whether they
cover the boundary behaviour required in this step.

Stage B: implement boundary adjustments. If `main` does not return
`eyre::Result<()>`, update it to do so. If domain logic should return
`podbot::error::Result<T>`, introduce a small helper (for example,
`fn run(cli: &Cli) -> podbot::error::Result<()>`) and map it to
`eyre::Result<()>` in `main`. Remove any non-test `unwrap`/`expect` calls by
returning errors instead. Keep public APIs stable outside the error module.

Stage C: tests. Add rstest unit tests to validate that converting each
`PodbotError` variant to an `eyre::Report` preserves the expected user-facing
message. Use parameterised cases for happy and unhappy paths (for example, a
successful stub returns `Ok(())`, and a missing configuration field reports a
clear error). Extend rstest-bdd scenarios to exercise the eyre boundary by
formatting a report and asserting its output for at least one success case and
one failure case, plus any relevant edge case (such as an empty error message
field).

Stage D: documentation and roadmap. Record any design decisions about the
error boundary in `docs/podbot-design.md`. Update `docs/users-guide.md` only if
user-visible error output or CLI behaviour changes. Mark Step 1.2 as done in
`docs/podbot-roadmap.md` once all validations pass.

## Concrete Steps

1. Inspect the entry point and error modules, and search for unwrap/expect:

    rg --hidden --line-number "unwrap\(|expect\(" src

2. Update `src/main.rs` to use `eyre::Result<()>` at the boundary and keep
   domain logic returning `podbot::error::Result<T>` if needed.

3. If any unwrap/expect calls exist outside tests, replace them with error
   returns or `?` propagation.

4. Add rstest unit coverage for the eyre boundary in a suitable test module
   (for example, `src/error.rs` or `tests/error_report.rs`). Use fixtures and
   parameterised cases for both happy and unhappy paths.

5. Extend behavioural tests:

    - Update `tests/features/error_handling.feature` with scenarios that
      mention the report boundary.
    - Update `tests/bdd_error_handling.rs` to format `eyre::Report` values and
      assert user-visible messages.

6. Update documentation:

    - `docs/podbot-design.md` for any new decisions about the error boundary.
    - `docs/users-guide.md` if user-facing error output changes.

7. Mark Step 1.2 as done in `docs/podbot-roadmap.md`.

8. Run validation commands with logs (use tee + pipefail):

    set -o pipefail
    make all | tee /tmp/podbot-make-all.log

   If documentation changed, also run:

    make fmt | tee /tmp/podbot-fmt.log
    MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint | \
      tee /tmp/podbot-markdownlint.log
    make nixie | tee /tmp/podbot-nixie.log

## Validation and Acceptance

Behavioural acceptance:

- The new rstest-bdd scenarios in `tests/features/error_handling.feature`
  execute via `cargo test` and assert the formatted `eyre::Report` output for
  at least one success case and one failure case, plus an edge case.
- Unit tests confirm that converting `PodbotError` variants into
  `eyre::Report` preserves the expected message content.

Quality criteria:

- Tests: `make all` passes; if documentation changed, `make fmt`,
  `make markdownlint`, and `make nixie` also pass.
- Lint/typecheck: `make all` includes `make lint` with warnings denied.
- Errors: No `unwrap` or `expect` calls appear outside test code.

## Idempotence and Recovery

All steps are repeatable. If a validation command fails, fix the issue and
re-run the same command; keep the log files so failures can be inspected. No
step should modify state outside the repository other than temporary log files.

## Artifacts and Notes

Capture short log excerpts that demonstrate success, such as:

    make all
    ...
    Finished `test` [unoptimized + debuginfo] target(s) in 0.42s
    Running unittests ... ok

## Interfaces and Dependencies

- `src/main.rs` must return `eyre::Result<()>` at the boundary.
- Domain modules should continue to return `podbot::error::Result<T>`.
- No new dependencies are expected; rely on existing `eyre` and `thiserror`.
- Tests should use `rstest` and `rstest-bdd` per existing patterns.
