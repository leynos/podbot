# Define The `run` Subcommand As A Library-Safe Adapter

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: DRAFT

Implementation must not begin until this plan is explicitly approved by the
user.

## Purpose / big picture

After this change, `podbot run` is a real, documented subcommand boundary
rather than an incidental Clap shape. Operators can invoke
`podbot run --repo owner/name --branch main` and receive clear parse-time or
semantic feedback, while Rust embedders can construct the same run request
through a versioned library API without importing `podbot::cli` or depending on
Clap types.

This plan is intentionally narrower than the later interactive orchestration
work in roadmap Step 6.2. The goal here is to make subcommand parsing and
routing correct, documented, and library-safe. The actual end-to-end
interactive container flow can remain delegated to the existing orchestration
stub until Step 6.2 is implemented.

Observable success for the eventual implementation:

1. `podbot run --repo owner/name --branch main` parses successfully and routes
   through a library-owned run request type instead of keeping repository and
   branch data trapped in CLI-only structs.
2. Missing required arguments still fail with clear Clap errors.
3. Hosted agent modes remain outside the `run` contract and are rejected with
   actionable guidance at the semantic validation layer.
4. A library consumer can construct the run request directly in Rust tests
   without importing `podbot::cli`.
5. `docs/podbot-design.md` records the run-request boundary decision,
   `docs/users-guide.md` documents the user-visible behaviour, and
   `docs/podbot-roadmap.md` marks only the specific
   `Define the run subcommand for launching agent sessions` task as done when
   implementation is complete.
6. `make check-fmt`, `make lint`, and `make test` all pass after the code
   change, with documentation gates passing as well.

## Repository orientation

The implementation work for this plan will centre on the existing CLI adapter,
the public API boundary, and the library-boundary test suite.

- `src/cli/mod.rs` already defines `Commands::Run(RunArgs)` and exposes
  `Cli::config_load_options()`. This is the current parse boundary.
- `src/main.rs` currently dispatches `Commands::Run(args)` to `run_agent_cli`,
  but the route still keeps `repo` and `branch` as CLI-local values and then
  calls `podbot::api::run_agent(config)` without a library-owned request.
- `src/api/mod.rs` exposes the current orchestration surface. This is where the
  library-facing run contract should live.
- `tests/library_embedding.rs`, `tests/bdd_library_boundary.rs`, and
  `tests/features/library_boundary.feature` already prove that public APIs can
  be exercised without CLI types. They should become the primary proof that the
  run request is library-owned.
- `src/cli/tests.rs`, `tests/bdd_cli.rs`, and `tests/features/cli.feature`
  already cover basic CLI parsing. They should be extended rather than replaced.
- `docs/users-guide.md` and `docs/podbot-design.md` already describe
  `podbot run` as a supported surface, so the implementation must reconcile
  documentation with the actual contract instead of introducing entirely new
  behaviour.

## Reference map

The implementer should keep these documents open while working:

- `docs/podbot-roadmap.md`: the authoritative scope for Phase 6, Step 6.1 and
  the exact roadmap checkbox that must be updated on completion.
- `docs/podbot-design.md`: the CLI surface, dual-delivery model, and the rule
  that library APIs must not depend on CLI-only types.
- `docs/users-guide.md`: the user-visible contract for `podbot run`.
- `docs/rust-testing-with-rstest-fixtures.md`: guidance for `rstest` fixtures
  and parameterized unit tests.
- `docs/rstest-bdd-users-guide.md`: guidance for `rstest-bdd` v0.5.0 feature
  files, step definitions, and fixture injection.
- `docs/reliable-testing-in-rust-via-dependency-injection.md`: guidance for
  keeping tests deterministic and avoiding live environment mutation.
- `docs/rust-doctest-dry-guide.md`: guidance if any public API docs gain new
  examples.
- `docs/complexity-antipatterns-and-refactoring-strategies.md`: use this if the
  routing or conversion logic starts to grow into a bumpy-road function.
- `docs/ortho-config-users-guide.md`: useful for preserving the separation
  between command parsing and layered configuration loading.

## Relevant skills

If the implementation is carried out in this environment, these skills are the
right companions for the work:

- `execplans`: keep this plan current as implementation progresses.
- `rust-router`: route Rust design and refactor questions to the smallest
  fitting Rust skill.
- `rust-types-and-apis`: shape the public `run` request and any library-facing
  conversion boundaries.
- `domain-cli-and-daemons`: keep the CLI as an adapter layer and avoid leaking
  process concerns into the library.

## Constraints

- The dual-delivery model described in `docs/podbot-design.md` must be
  preserved. The CLI is an adapter; the library owns the semantic contract.
- No public library API may accept or return Clap types.
- `podbot::cli` must remain behind the existing `cli` feature gate.
- This plan covers Step 6.1 task `Define the run subcommand for launching agent
  sessions
  `. It must not silently absorb the Step 6.2 interactive orchestration implementation.
- `podbot run` remains the interactive entry point. Hosted modes belong to
  `podbot host`, and the implementation must preserve that separation.
- Unit coverage must use `rstest`.
- Behavioural coverage must use `rstest-bdd` v0.5.0.
- Tests must cover happy paths, unhappy paths, and relevant edge cases.
- Documentation updates must use en-GB-oxendict spelling and must update both
  `docs/podbot-design.md` and `docs/users-guide.md` when behaviour or design
  decisions change.
- When the feature is complete, update only the relevant roadmap checkbox in
  `docs/podbot-roadmap.md`; do not mark the entire Step 6.1 section complete
  unless all Step 6.1 tasks are finished.
- Quality gates must run sequentially through `tee` logs because concurrent
  Cargo invocations in this repo can block on build locks.

## Tolerances (exception triggers)

- If satisfying the no-CLI-coupling requirement needs breaking changes beyond
  the single run-specific public API boundary, stop and ask for confirmation.
- If the work expands beyond roughly 12 files or 800 net lines of code, stop
  and reassess whether the scope has drifted into Step 6.2 or Phase 4 launch
  planning work.
- If implementation appears to require a new dependency, stop and ask before
  adding it.
- If behavioural coverage requires replacing the existing CLI BDD harness
  instead of extending it, stop and justify the replacement before proceeding.
- If modifying `.feature` files causes stale compile-time scenario output,
  clean with `cargo clean -p podbot` once. If tests still behave inconsistently
  after that, stop and investigate before continuing.

## Risks

- Risk: the current public `run_agent(&AppConfig)` signature cannot represent
  `repo` and `branch`, so leaving it unchanged would preserve CLI coupling by
  omission. Mitigation: introduce a library-owned run request and update
  boundary tests to construct it directly.

- Risk: parse-time validation and semantic validation can blur together, which
  produces confusing operator errors. Mitigation: keep required-flag failures
  in Clap, and use semantic validation for mode legality such as hosted modes
  passed to `run`.

- Risk: the documentation already presents `podbot run` as established
  behaviour, so implementation details may accidentally contradict existing
  prose. Mitigation: update `docs/users-guide.md` and `docs/podbot-design.md`
  in the same change as the code and tests.

- Risk: library-boundary tests currently treat run orchestration as a stub, so
  they may miss the new request contract unless they are extended deliberately.
  Mitigation: add both `rstest` integration coverage and `rstest-bdd`
  behavioural coverage for the run request specifically.

## Implementation plan

1. Establish the library-owned run request boundary.

   Add a `RunRequest` or equivalently named value type under the public API
   surface in `src/api/`, holding the subcommand-specific inputs that are
   currently CLI-only, starting with repository and branch. The public run
   orchestration entry point should accept this request alongside `AppConfig`,
   so embedders can launch a run session without importing `podbot::cli`.

   Recommended file targets:

   - `src/api/mod.rs`
   - a new `src/api/run.rs` if extracting the request and routing logic keeps
     modules smaller and clearer
   - `src/api/tests.rs`
   - `tests/library_embedding.rs`
   - `tests/bdd_library_boundary.rs`
   - `tests/bdd_library_boundary_helpers/steps.rs`
   - `tests/features/library_boundary.feature`

2. Tighten the CLI adapter around that request boundary.

   Keep `src/cli/mod.rs` responsible only for parsing and for converting
   `RunArgs` into the library-owned request. `src/main.rs` should stop treating
   `repo` and `branch` as ad hoc values passed around the binary layer and
   instead call the public API with the converted request.

   The implementation should keep `run` parsing focused:

   - `--repo` remains required.
   - `--branch` remains required.
   - `--agent` remains an optional override.
   - `--agent-mode` continues to exist as an override path, but semantic
     validation must preserve the rule that `run` is interactive-only.

   Recommended file targets:

   - `src/cli/mod.rs`
   - `src/cli/tests.rs`
   - `src/main.rs`

3. Extend unit coverage with `rstest`.

   Add or update `rstest` cases to prove:

   - the CLI converts `RunArgs` into the library-owned request correctly,
   - `Cli::config_load_options()` still carries command intent and overrides
     correctly for `run`,
   - the public API accepts the request without CLI types,
   - any intentional public API evolution is reflected in the library-embedding
     tests.

   Prefer parameterized cases over near-duplicate tests. If new helper setup is
   shared, express it as fixtures rather than inline repetition.

4. Extend behavioural coverage with `rstest-bdd` v0.5.0.

   Update `tests/features/cli.feature` and `tests/bdd_cli.rs` so the CLI
   scenarios cover:

   - the happy path for `run --repo owner/name --branch main`,
   - missing `--repo`,
   - missing `--branch`,
   - the help surface mentioning `run` and its required arguments where
     relevant.

   Update `tests/features/library_boundary.feature` and its step definitions so
   a library consumer can construct and pass the run request directly. Keep the
   BDD state model aligned with the existing
   `StepResult<T> = Result<T, String>` style used across the repo.

5. Update documentation and roadmap state.

   Record the boundary decision in `docs/podbot-design.md`: `run` is a CLI
   adapter over a library-owned request type, not a Clap-only concept. Update
   `docs/users-guide.md` so the user-facing contract matches the implemented
   argument and validation behaviour. When the implementation is complete,
   change only the `Define the run subcommand for launching agent sessions`
   checkbox in `docs/podbot-roadmap.md` to done.

6. Run the full validation sequence and capture evidence.

   After code and documentation changes are complete, run these commands
   sequentially:

   ```bash
   set -o pipefail && make fmt 2>&1 | tee /tmp/podbot-fmt.log
   set -o pipefail && MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint 2>&1 | tee /tmp/podbot-markdownlint.log
   set -o pipefail && make nixie 2>&1 | tee /tmp/podbot-nixie.log
   set -o pipefail && make check-fmt 2>&1 | tee /tmp/podbot-check-fmt.log
   set -o pipefail && make lint 2>&1 | tee /tmp/podbot-lint.log
   set -o pipefail && make test 2>&1 | tee /tmp/podbot-test.log
   ```

   Expected outcomes:

   - formatting and Markdown validation pass,
   - clippy and docs linting pass,
   - the full workspace test suite passes,
   - the updated `run` unit and behavioural tests fail before the change and
     pass after it.

## Progress

- [x] 2026-04-21: Reviewed `docs/podbot-roadmap.md`,
  `docs/podbot-design.md`, `docs/users-guide.md`, the existing CLI adapter, the
  public API boundary, and the current unit and BDD harnesses.
- [x] 2026-04-21: Drafted this ExecPlan for approval.
- [ ] Stage A: Introduce the library-owned run request.
- [ ] Stage B: Route the CLI through the request boundary.
- [ ] Stage C: Extend `rstest` coverage.
- [ ] Stage D: Extend `rstest-bdd` coverage.
- [ ] Stage E: Update design, user, and roadmap documentation.
- [ ] Stage F: Run format, lint, and test gates successfully.

## Surprises & Discoveries

- Discovery: the repository already contains a provisional `run` subcommand in
  `src/cli/mod.rs` and `src/main.rs`, so this is a contract-hardening task, not
  a greenfield CLI addition.
- Discovery: `podbot::api::run_agent` currently accepts only `&AppConfig`,
  which means repository and branch data still live exclusively in CLI parse
  types. That violates the spirit of the completion criterion for embedders.
- Discovery: `docs/users-guide.md` already documents `run` and required flags,
  so documentation updates must reconcile existing claims with the refined API
  boundary.
- Discovery: the repo already pins `rstest-bdd = "0.5.0"` in `Cargo.toml`, so
  no dependency change is needed for the required behavioural coverage.

## Decision Log

- Decision: this plan treats the Step 6.1 run task as a boundary-design change,
  not merely a Clap syntax tweak. Rationale: the roadmap completion criterion
  and the design doc both require a library surface with no CLI coupling
  requirements. Date/Author: 2026-04-21 / Codex

- Decision: the implementation should introduce a library-owned run request
  instead of keeping repository and branch values trapped inside `RunArgs`.
  Rationale: a Rust embedder cannot launch a repo-targeted run session through
  the public API otherwise. Date/Author: 2026-04-21 / Codex

- Decision: this plan does not include the full interactive launch flow from
  Step 6.2. Rationale: combining dispatch hardening and orchestration would
  blur roadmap boundaries, inflate scope, and make approval less precise.
  Date/Author: 2026-04-21 / Codex

## Outcomes & Retrospective

No implementation has started. This section must be rewritten after approval
and completion to record what shipped, what changed from the draft, and what
follow-up work remains.
