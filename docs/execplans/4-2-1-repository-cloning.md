# Step 4.2.1: Clone the requested repository into the sandbox workspace

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: DRAFT

This plan is draft-only. Implementation must not begin until the user
explicitly approves this document.

## Purpose and big picture

Complete roadmap Step 4.2 ("Repository cloning") by introducing a tested,
library-owned repository-cloning slice that can clone `owner/name` into the
configured `workspace.base_dir` inside the sandbox container with the requested
branch checked out and with authentication flowing through `GIT_ASKPASS`
instead of credentials embedded in command arguments.

After this change, Podbot will have a concrete, reusable repository preparation
primitive that:

- accepts repositories in `owner/name` form and rejects malformed inputs;
- requires an explicit branch value with no default branch guessing;
- invokes `git clone` inside the running container with
  `GIT_ASKPASS=<helper-path>` and no token in the URL or argv;
- clones into `workspace.base_dir` as the workspace root for
  `workspace.source = "github_clone"`;
- verifies that the clone succeeded and that the requested branch is the
  checked-out branch.

Observable success when this plan is implemented:

- the new unit tests built with `rstest` pass for happy paths, unhappy paths,
  and edge cases;
- the new behavioural tests built with `rstest-bdd` v0.5.0 pass for clone
  success, malformed repository input, and clone failure paths;
- `make check-fmt`, `make lint`, and `make test` all pass;
- `docs/podbot-design.md`, `docs/users-guide.md`, and
  `docs/podbot-roadmap.md` are updated to reflect the implemented behaviour.

This step deliberately stops at repository preparation. It does not claim that
full interactive agent startup is complete; that remains the responsibility of
Step 4.3.

## Constraints

Hard invariants that must hold throughout implementation. Violation requires
escalation, not workarounds.

- Files must remain below 400 lines each.
- Every Rust module must begin with a `//!` module-level doc comment.
- Use en-GB-oxendict spelling in code comments and documentation.
- Do not expose GitHub installation tokens in process arguments, shell history,
  or logged URLs.
- Do not add a new external dependency.
- Do not move repository parsing into clap-only types; the library surface must
  own repository and branch validation for embedders as well as the CLI.
- Keep `workspace.source = "github_clone"` semantics aligned with
  `docs/podbot-design.md`: clone into `workspace.base_dir` inside the container.
- Do not silently guess a branch. The implementation must require the caller to
  provide one.
- Use `rstest` for unit coverage and `rstest-bdd` v0.5.0 for behavioural
  coverage.
- Behavioural test steps must use the repository’s `StepResult<T> = Result<T,
  String>` pattern and must not use `expect()` or `panic!()
  ` for ordinary failure paths.
- Library code must not print to stdout or stderr and must not call
  `std::process::exit`.
- All changed documentation must be formatted and linted.

## Tolerances (exception triggers)

- Scope: if landing Step 4.2 requires changes to more than 18 files or roughly
  750 net lines of code, stop and escalate with a narrowed alternative.
- Interface: if implementation requires a breaking change to the stable public
  library surface beyond the run-orchestration entry point for repository and
  branch input, stop and escalate with options.
- Dependencies: if Step 4.2 cannot consume the Step 3.2/3.4 token and
  `GIT_ASKPASS` contract without reimplementing those steps, stop and either
  land the missing prerequisite first or request approval to widen scope.
- Iterations: if the same failing test or lint issue survives three focused
  fix attempts, stop and document the blocker.
- Ambiguity: if `workspace.base_dir` cannot be treated as the exact clone
  destination without contradicting other accepted documentation, stop and ask
  for direction before changing semantics.

## Risks

- Risk: Step 3.2 (installation token acquisition) and Step 3.4
  (`GIT_ASKPASS` contract) are roadmap prerequisites, but their implementation
  is not present yet in this tree. Severity: high. Likelihood: high.
  Mitigation: treat those steps as required inputs to Step 4.2; if they are
  still missing at implementation time, land or merge them first rather than
  improvising a second authentication path.

- Risk: `workspace.base_dir` reads like a parent directory name, but the design
  document says Podbot clones into that path directly. Severity: medium.
  Likelihood: medium. Mitigation: document the chosen interpretation in
  `docs/podbot-design.md` and verify it in tests with explicit path
  expectations.

- Risk: repository validation that is too strict may reject valid GitHub
  slugs. Severity: medium. Likelihood: medium. Mitigation: validate only the
  required structure (`owner/name`, exactly one slash, both segments non-empty,
  trimmed input) instead of inventing a narrow allowlist.

- Risk: partial `run_agent` wiring could make the CLI appear more complete than
  it really is before Step 4.3. Severity: medium. Likelihood: medium.
  Mitigation: keep Step 4.2 focused on repository-preparation APIs and their
  tests, and only extend the top-level `run` path as far as is honest and
  documented.

- Risk: `.feature` file edits may appear to be ignored because
  `rstest-bdd` reads features at compile time. Severity: low. Likelihood:
  medium. Mitigation: if a behavioural test appears stale after editing a
  feature file, run `cargo clean -p podbot` before rerunning `make test`.

## Progress

- [x] 2026-04-22: Drafted the ExecPlan and captured current repository
  constraints, code seams, and prerequisite dependencies.
- [ ] Await user approval of this plan.
- [ ] Add library-owned repository and branch request/value types.
- [ ] Add engine-level repository-cloning helper that uses `GIT_ASKPASS`.
- [ ] Add API-level wrapper and update any public API tests that cover the new
  clone surface.
- [ ] Add `rstest` unit coverage and `rstest-bdd` behavioural coverage.
- [ ] Update `docs/podbot-design.md`, `docs/users-guide.md`, and
  `docs/podbot-roadmap.md`.
- [ ] Run `make fmt`, `make markdownlint`, `make nixie`, `make check-fmt`,
  `make lint`, and `make test`.

## Surprises and discoveries

- `src/cli/mod.rs` already requires `--branch` for `podbot run`; Step 4.2 does
  not need to invent or restore that requirement.
- `src/engine/connection/exec/mod.rs` already exposes `ExecRequest::with_env`,
  so the clone flow can pass `GIT_ASKPASS` and `GIT_TERMINAL_PROMPT=0` through
  the existing exec seam without widening the daemon protocol.
- `src/api/mod.rs::run_agent` is still a stub and does not currently accept
  repository or branch input, so repository cloning cannot be implemented only
  by filling in the existing stub body.
- No Step 3.2 or Step 3.4 ExecPlan file is present under `docs/execplans/`,
  even though Step 4.2 depends on those contracts being available.

## Decision log

- Decision: model repository input as a library-owned validated value object
  rather than a raw CLI string. Rationale: the roadmap promises both CLI and
  library delivery surfaces. Structural validation belongs in shared library
  code, not only in clap. A small `RepositoryRef` newtype with `owner()` and
  `name()` accessors keeps the clone flow free from raw-string parsing.

- Decision: validate repository syntax structurally, not by imposing a custom
  GitHub-specific character whitelist. Rationale: the task requires
  `owner/name` format, not a full GitHub slug validator. Enforcing exactly one
  slash and non-empty segments is enough to catch malformed operator input
  without rejecting legitimate repositories.

- Decision: treat `workspace.base_dir` as the exact clone destination for
  `workspace.source = "github_clone"`. Rationale: `docs/podbot-design.md` says
  Podbot clones into that path and treats it as the workspace root. Step 4.4
  can revisit broader workspace-path policy, but Step 4.2 should not invent a
  second destination convention such as `base_dir/<repo-name>`.

- Decision: reuse the existing container exec seam with explicit environment
  variables instead of shell-interpolating credentials. Rationale:
  `ExecRequest::with_env` already supports safe environment injection. The
  clone command can therefore stay as direct argv (`git clone ...`) while
  `GIT_ASKPASS` points at the helper script and the token stays off argv.

- Decision: add a dedicated repository-cloning test slice instead of inflating
  the generic orchestration BDD suite. Rationale: the existing
  `tests/bdd_orchestration.rs` file currently covers exec and stub
  orchestration only. Repository cloning has distinct happy and unhappy paths
  and deserves its own feature file, helpers, and assertions.

- Decision: do not claim Step 4.3 agent-startup completion inside Step 4.2.
  Rationale: repository preparation is a prerequisite for agent launch, but it
  is not the same thing. If the implementation needs a small amount of
  `run_agent` reshaping to carry repository input, keep that reshaping honest
  and explicitly documented as pre-launch plumbing.

## Outcomes and retrospective

To be completed after implementation. Record what landed, which tolerances were
consumed, whether Step 3.x prerequisites required adjustment, and what should
be simplified before Step 4.3.

## Context and orientation

Podbot is a Rust project with a thin CLI adapter over a library boundary. The
current repository state already contains most of the low-level building blocks
that Step 4.2 should consume, but not the repository-cloning slice itself.

Relevant current code:

- `src/cli/mod.rs`
  - `RunArgs` already contains `repo: String` and `branch: String`.
  - `--branch` is already required and has no default.
- `src/main.rs`
  - `run_agent_cli(...)` still prints a stub message and calls
    `podbot::api::run_agent(config)` without repository or branch input.
- `src/api/mod.rs`
  - `run_agent(&AppConfig)` is still a stub returning `CommandOutcome::Success`.
- `src/engine/connection/exec/mod.rs`
  - `ExecRequest` already supports `with_env`, which is the existing seam for
    `GIT_ASKPASS`.
- `src/config/workspace.rs`
  - `WorkspaceConfig` already carries `workspace.base_dir` and defaults it to
    `"/work"`.
- `src/config/validation.rs`
  - `workspace.source = "github_clone"` validation already rejects
    host-mount-only fields.
- `src/engine/connection/git_identity/`
  - Step 4.1 established the pattern of adding a focused engine helper plus a
    separate API wrapper instead of immediately claiming full `run_agent`
    orchestration.

The design document sections that matter most are:

- `docs/podbot-design.md`, "Execution flow", step 6 (`github_clone`) for the
  clone destination and authentication approach.
- `docs/podbot-design.md`, token refresh section, for the mounted token file
  and helper-script contract consumed by `GIT_ASKPASS`.
- `docs/podbot-roadmap.md`, Step 4.2 for the task list and completion
  criteria.

## Relevant documentation and skills

Implementation of this plan should keep the following documents open:

- `docs/podbot-roadmap.md`
- `docs/podbot-design.md`
- `docs/users-guide.md`
- `docs/rust-testing-with-rstest-fixtures.md`
- `docs/rstest-bdd-users-guide.md`
- `docs/reliable-testing-in-rust-via-dependency-injection.md`
- `docs/complexity-antipatterns-and-refactoring-strategies.md`
- `docs/ortho-config-users-guide.md`
- `docs/rust-doctest-dry-guide.md`
- `docs/execplans/4-1-1-git-identity-configuration.md`

Relevant skills for the implementer:

- `execplans`
  - Keep this document current as work progresses.
- `leta`
  - Use semantic symbol navigation instead of line-oriented browsing when
    tracing `run_agent`, `RunArgs`, `ExecRequest`, and related tests.
- `rust-router`
  - Route follow-up questions to the smallest Rust skill needed.
- `rust-errors`
  - Keep repository-validation and clone-failure reporting semantic and typed.
- `domain-cli-and-daemons`
  - Reason clearly about the boundary between the CLI adapter, long-lived token
    state, and in-container Git execution.

## Plan of work

### Milestone A: Confirm prerequisites and define the request model

Start by confirming which Step 3.x surfaces already exist in the branch being
implemented. Step 4.2 must consume, not reinvent, the token and `GIT_ASKPASS`
contract. If token acquisition, token mounting, or the helper-script path are
still absent, stop and either land those prerequisites first or request
approval to widen scope.

Once the prerequisites are clear, add library-owned request/value types for
repository preparation. The exact file name can be chosen during
implementation, but keep the code split small and coherent. One plausible shape
is:

- `src/api/run_agent.rs` for higher-level orchestration request types and
  wrappers, or `src/api/repository_clone.rs` if the clone slice is kept
  separate from full `run_agent`.
- `src/api/mod.rs` to re-export the new library surface.
- `src/cli/mod.rs` and `src/cli/tests.rs` only for CLI-facing validation
  and adapter glue.

Define a validated repository type, for example `RepositoryRef`, that
represents `owner/name` and rejects malformed inputs before any engine call is
made. If a separate branch value object is useful, keep it intentionally small:
accept a non-empty trimmed string and avoid a speculative full Git ref parser
in this step.

At the end of this milestone, the library API should have a concrete way to
carry repository and branch input without relying on clap parse types or raw
strings scattered through the codebase.

### Milestone B: Add the engine-level repository clone helper

Create a new connection submodule that mirrors the style of `git_identity/` and
`upload_credentials/`. A suitable home is:

- `src/engine/connection/repository_clone/mod.rs`
- `src/engine/connection/repository_clone/tests.rs`

If helpers are needed to stay below the 400-line limit, split them into small
files such as `request.rs`, `verification.rs`, or `test_helpers.rs`.

The engine-level helper should:

1. Accept the running container ID, the validated repository reference, the
   explicit branch, the configured `workspace.base_dir`, the runtime handle,
   and the injected `ContainerExecClient`.
2. Build the GitHub HTTPS remote as
   `https://github.com/<owner>/<name>.git` without embedding credentials.
3. Run clone preparation in direct argv form, not through `sh -c`, unless a
   direct argv approach proves impossible. If a shell becomes necessary,
   document why in the `Decision log` before proceeding.
4. Pass `GIT_ASKPASS=<helper-path>` and `GIT_TERMINAL_PROMPT=0` via
   `ExecRequest::with_env`.
5. Clone into `workspace.base_dir` as the exact destination path.
6. Verify success with a second deterministic Git command, such as
   `git -C <workspace.base_dir> rev-parse --abbrev-ref HEAD`, and compare the
   result to the requested branch.
7. Return a typed success result that reports the resolved workspace path and
   checked-out branch, or a semantic error mapped through existing Podbot
   errors.

Keep clone verification simple and observable. Do not over-design a large
workspace model in this step; Step 4.4 is the place for broader workspace
strategy normalisation.

### Milestone C: Add the library wrapper and minimal adapter wiring

Expose the new repository-cloning capability through the library boundary so it
is usable from both the CLI and embedders. There are two acceptable paths, and
the implementer should choose the smaller one that preserves clarity:

1. Add a focused public API such as `clone_repository_into_workspace(...)` and
   keep `run_agent` honest as a later Step 4.3 concern.
2. If the branch already intends to reshape `run_agent`, add a library-owned
   request type and update `run_agent` to consume it, but stop before claiming
   full agent startup.

Prefer the first path unless there is already approved work that reshapes
`run_agent`, because it avoids a premature promise that `podbot run` is fully
implemented.

Whichever path is chosen, update the existing public-surface tests:

- `src/api/tests.rs`
- `tests/library_embedding.rs`
- `tests/bdd_library_boundary.rs`
- `tests/bdd_library_boundary_helpers/`

If the public API signature changes, keep the change tightly scoped and record
it in `Decision log`. If the new shape would broaden the stable library surface
more than this milestone needs, stop and escalate.

### Milestone D: Add unit and behavioural tests

Unit coverage should prove both validation and clone orchestration without
requiring a live daemon. Use `rstest` fixtures and a mocked
`ContainerExecClient` to cover:

- valid `owner/name` parsing;
- malformed repository strings such as missing slash, empty owner, empty name,
  and extra slash;
- empty or whitespace-only branch rejection;
- clone command construction, including destination path and branch flags;
- environment propagation for `GIT_ASKPASS` and `GIT_TERMINAL_PROMPT=0`;
- verification success when the inspected branch matches;
- failure mapping when clone or verification exec commands return non-zero.

Add a dedicated behavioural suite rather than extending the generic
orchestration scenarios:

- `tests/bdd_repository_cloning.rs`
- `tests/bdd_repository_cloning_helpers/mod.rs`
- `tests/bdd_repository_cloning_helpers/state.rs`
- `tests/bdd_repository_cloning_helpers/steps.rs`
- `tests/bdd_repository_cloning_helpers/assertions.rs`
- `tests/features/repository_cloning.feature`

Required behavioural scenarios:

- happy path: a valid repository and branch produce a clone request that uses
  the helper environment and reports success;
- unhappy path: malformed repository input fails before any engine exec is
  attempted;
- unhappy path: clone exec failure returns a semantic error;
- unhappy path: verification discovers the wrong checked-out branch and fails
  deterministically.

If feature-file edits do not appear in the test run, use the repo’s known
workaround and run `cargo clean -p podbot` once before rerunning `make test`.

### Milestone E: Update design and user-facing documentation

When the code and tests are complete, update the documentation in the same
change:

- `docs/podbot-design.md`
  - Record the exact `workspace.base_dir` semantics for `github_clone`.
  - Record that authentication uses `GIT_ASKPASS` and no token-bearing clone
    URL.
- `docs/users-guide.md`
  - Document that `podbot run --repo` expects `owner/name`.
  - Clarify that `--branch` is required and has no default.
  - Clarify the clone destination semantics for `workspace.base_dir`.
- `docs/podbot-roadmap.md`
  - Mark the Step 4.2 tasks and completion criteria as done once the feature is
    actually complete.

Keep the documentation aligned with what the code truly does. Do not describe
full agent startup in Step 4.2 documentation if Step 4.3 remains unfinished.

### Milestone F: Validate and prepare for review

Run the full validation stack sequentially, capturing logs with `tee` as
required by the repository instructions.

```plaintext
set -o pipefail && make fmt 2>&1 | tee /tmp/podbot-fmt.log
set -o pipefail && MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint \
  2>&1 | tee /tmp/podbot-markdownlint.log
set -o pipefail && make nixie 2>&1 | tee /tmp/podbot-nixie.log
set -o pipefail && make check-fmt 2>&1 | tee /tmp/podbot-check-fmt.log
set -o pipefail && make lint 2>&1 | tee /tmp/podbot-lint.log
set -o pipefail && make test 2>&1 | tee /tmp/podbot-test.log
```

If `make lint` or `make test` appears to hang while waiting on the Cargo build
directory, inspect for a stale background Leta `cargo check` process and stop
that process before rerunning the gate. Run the gates sequentially, not in
parallel.

Before requesting merge or committing, update this ExecPlan:

- mark completed progress items;
- record any surprises or deviations from the draft;
- note the final public API shape in `Decision log`;
- fill in `Outcomes and retrospective`;
- confirm that the roadmap entry was marked done only after the feature and its
  documentation truly landed.

## Approval checkpoint

This plan is ready for review, but it is not approved for implementation yet.
Wait for explicit user approval before making any code or documentation changes
outside this ExecPlan itself.
