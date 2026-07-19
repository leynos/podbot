# Route `host`, `token-daemon`, `ps`, and `stop` through library-owned requests

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: DRAFT

This plan completes roadmap task 6.1.1b. The `run` and `exec` subcommands
already route through library-owned request types (see
`docs/execplans/6-1-1-run-subcommand.md` for `run`, and the existing
`podbot::api::ExecRequest` surface for `exec`). The four remaining operator
subcommands — `host`, `token-daemon`, `ps`, and `stop` — currently parse
through Clap but hand raw values (or no value at all) to library stubs, and
the dispatch layer prints status banners directly via `println!`. This plan
hardens those dispatch paths so each subcommand has a stable library-owned
request boundary, matching the pattern established by `RunRequest` and
`ExecRequest`. The plan stops short of implementing the actual orchestration
for any of those commands: that work lives in roadmap steps 6.3.1
(`ps`/`stop` real listings), 6.4.1 (`token-daemon` lifecycle), and 6.5.1
(`podbot host` protocol hosting).

## Purpose / big picture

After this change, every operator subcommand on the `podbot` binary parses
arguments via Clap and dispatches through a library-owned request type. The
CLI binary remains a disciplined adapter: no subcommand keeps its inputs
trapped in `clap`-only structs, and no subcommand writes diagnostics in a
way that would corrupt the future protocol-hosting stdout.

Observable success once the implementation completes:

1. Running `podbot host --agent codex --agent-mode codex_app_server` (or
   `--agent-mode acp`) parses through Clap, builds a library-owned
   `podbot::api::HostRequest`, and dispatches through a library entry
   point. Until the orchestration in roadmap step 6.5.1 lands, the
   library returns a documented "hosting not yet available" semantic
   error, and the CLI emits zero bytes to stdout before exiting.
2. Running `podbot token-daemon <container-id>` parses through Clap,
   builds a `podbot::api::TokenDaemonRequest`, and routes through the
   library API.
3. Running `podbot ps` and `podbot stop <container>` parse through Clap and
   route through library-owned request types
   (`podbot::api::ListContainersRequest` and `podbot::api::StopRequest`).
4. A library consumer can construct each request directly in Rust without
   importing `podbot::cli` or any Clap types.
5. `podbot --help`, `podbot host --help`, `podbot token-daemon --help`,
   `podbot ps --help`, and `podbot stop --help` all describe each command
   with its required flags and positional arguments.
6. Invalid invocations (for example, `podbot host` without `--agent-mode`
   when no configuration default is available, or `podbot host` with
   `--agent-mode podbot`) fail with clear Clap parse errors or clear
   semantic library errors. Operator guidance never appears on stdout for
   `host`.
7. Library boundary tests prove that each request type can be constructed
   without `podbot::cli` and that semantic validation rejects illegal
   combinations.
8. `docs/podbot-design.md`, `docs/users-guide.md`, and
   `docs/podbot-roadmap.md` reflect the new boundary. The roadmap entry
   for 6.1.1b lists `host`, `token-daemon`, `ps`, and `stop` as done while
   leaving 6.2.x through 6.5.x untouched.
9. `make check-fmt`, `make lint`, and `make test` all pass after the
   change, and `make markdownlint` plus `make nixie` pass for the
   documentation updates.

This plan is intentionally narrower than roadmap step 6.2 (interactive
launch through `podbot run`), step 6.3 (real `ps`, `stop`, and `exec`
orchestration), step 6.4 (operator-supervised token daemon), and step
6.5 (protocol-only `podbot host`). The goal here is to make subcommand
parsing, request construction, and routing correct, documented, and
library-safe.

## Repository orientation

The implementation will touch the existing CLI adapter, the public API
boundary, the binary entry point, and the existing library-boundary test
suite. The mental model: Clap parses; `cli::*Args::to_*Request()` adapts
to the library type; `podbot::api::*` owns semantics and validation;
`main.rs::*_cli` wraps the library call with operator-facing
presentation. For `host`, the wrapper must emit zero bytes to stdout
before the library returns.

- `src/cli/mod.rs` defines `Commands::{Run, Host, TokenDaemon, Ps, Stop,
  Exec}` and the associated `*Args` parse structs. `RunArgs` already
  carries a `to_run_request()` conversion that returns a library
  `RunRequest`. The plan adds the analogous methods on `HostArgs`,
  `TokenDaemonArgs`, and `StopArgs`. `Commands::Ps` carries no
  positional or flag data today, so its conversion is a constructor
  call with no arguments.
- `src/main.rs` dispatches subcommands. Today its `run` helper hard-codes
  the `host` branch to return a `ConfigError::InvalidValue` and routes
  the others through `*_api()` thunks that re-enter the library behind
  a `feature = "experimental"` gate. `host_agent_cli` is dead-coded with
  `#[expect(dead_code)]` and uses `println!`. The plan replaces the
  dead-coded helper with a library-driven dispatch that emits only to
  stderr, restores the `Commands::Host(args)` arm to convert and call
  the library function, and tightens the other `*_cli` wrappers to use
  the new request types.
- `src/api/mod.rs` exposes the stable boundary. The plan re-exports new
  request types from `api`. The new library functions remain feature-gated
  on `experimental` for orchestration parity with the existing stubs;
  they return a documented "not yet implemented" semantic error today
  until the real orchestration arrives in later roadmap steps. The
  request types themselves are stable and live outside the `experimental`
  gate so embedders can construct them at compile time.
- `src/api/run.rs` is the template for the new request modules. Each
  new request type lives in its own module under `src/api/`
  (`host.rs`, `token_daemon.rs`, `stop.rs`, and either `ps.rs` or an
  inline unit type in `mod.rs`) so module size stays under the 400-line
  cap.
- `tests/features/cli.feature` and `tests/bdd_cli.rs` cover Clap parse
  behaviour for each subcommand. The plan adds scenarios for `host`,
  `token-daemon`, `ps`, and `stop` parse success and parse failures, and
  extends help-text coverage.
- `tests/features/library_boundary.feature`,
  `tests/bdd_library_boundary.rs`, and
  `tests/bdd_library_boundary_helpers/` prove that public APIs are
  reachable without CLI types. The plan extends them with scenarios that
  build each new request type and call the library entry point.
- `tests/library_embedding.rs` is the `rstest` suite that exercises the
  stable library surface from an embedder's perspective. The plan adds
  parameterized cases for each new request type's constructor,
  semantic validation, and stub call.
- `docs/podbot-design.md` documents the dual-delivery model and the
  library API reference. The plan records the new request types in the
  public library API reference and notes that the orchestration entry
  points remain experimental.
- `docs/users-guide.md` documents the CLI surface for operators. The
  plan reconciles the `host`, `token-daemon`, `ps`, and `stop` entries
  with the new boundary, including the stderr-only diagnostics rule for
  `host`.
- `docs/podbot-roadmap.md` carries the checkbox list for 6.1.1b. The
  plan only ticks the four newly delivered subcommand checkboxes.

## Reference map

The implementer should keep these documents and references open while
working:

- `docs/podbot-roadmap.md` — the authoritative scope for Phase 6, Step
  6.1, and the exact roadmap checkboxes that must change on completion.
- `docs/podbot-design.md` — the CLI surface, dual-delivery model, ACP
  capability masking contract, and the rule that library APIs must not
  depend on CLI-only types.
- `docs/users-guide.md` — the user-visible contract for each operator
  subcommand.
- `docs/developers-guide.md` — the protocol stdout-purity rule
  ("no banners, progress indicators, or status messages on stdout" for
  protocol mode and `podbot host`), and the public API boundary
  reference.
- `docs/execplans/6-1-1-run-subcommand.md` — the prior plan that
  established the `RunRequest` pattern. Treat it as the template; this
  plan extends the same boundary to four further subcommands.
- `docs/execplans/5-1-1-public-orchestration-module.md` — the prior
  extraction work that introduced the `api` module and the
  experimental gating.
- `docs/execplans/5-3-1-stabilize-public-library-boundaries.md` — the
  policy for what counts as a stable versus experimental public surface.
- `docs/rust-testing-with-rstest-fixtures.md` — guidance for shared
  fixtures and parameterized unit tests.
- `docs/rstest-bdd-users-guide.md` — guidance for `rstest-bdd` v0.5.0
  feature files, step definitions, and `ScenarioState` plumbing.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` —
  guidance for keeping tests deterministic and avoiding live
  environment mutation.
- `docs/rust-doctest-dry-guide.md` — guidance for documentation
  examples that compile.
- `docs/complexity-antipatterns-and-refactoring-strategies.md` — apply
  if any conversion or dispatch helper grows into a bumpy-road
  function.
- `docs/ortho-config-users-guide.md` — preserves the separation between
  CLI parsing and layered configuration loading.
- Jujutsu's "Separation of library from UI" architecture note
  (`docs.jj-vcs.dev/latest/technical/architecture/#separation-of-library-from-ui`)
  for cited prior art on a library/CLI split mandate.
- The Model Context Protocol stdio transport specification
  (`modelcontextprotocol.io/specification/2025-06-18/basic/transports`,
  §"stdio") for the cited MUST that the server "MUST NOT write anything
  to its `stdout` that is not a valid MCP message". This is the
  external grounding for why `podbot host` dispatch may not print
  banners to stdout.
- Alexis Lozano's "Hexagonal architecture in Rust" series, part 6
  (`alexis-lozano.com/blog/hexagonal-architecture-in-rust-6/`) for a
  Rust-flavoured CLI-as-adapter example calling library-owned
  `domain::*::Request` values, including the rule that semantic
  validation lives at the library boundary, not in the adapter.

## Relevant skills

If the implementation is carried out in this environment, these skills
are the right companions for the work:

- `execplans`: keep this plan current as implementation progresses.
- `rust-router`: route Rust design and refactor questions to the
  smallest fitting Rust skill.
- `rust-types-and-apis`: shape each public request type and the
  library-facing conversion boundary. Reach for newtype patterns only
  if validation behaviour is non-trivial; the existing `RunRequest` and
  `ExecRequest` already use plain structs, and consistency is more
  valuable than premature abstraction here.
- `rust-errors`: keep semantic error variants thin, additive, and
  matched to the existing `ConfigError`/`PodbotError` taxonomy.
  Re-use `ConfigError::MissingRequired` and `ConfigError::InvalidValue`
  unless an existing variant cannot represent the failure.
- `domain-cli-and-daemons`: keep the CLI as a thin adapter and avoid
  leaking process concerns into the library. The library must not
  call `std::process::exit` or write to stdout/stderr.
- `hexagonal-architecture`: defend the boundary in the right place. The
  ports here are the `api::*Request` types and the
  `api::host_agent`/`api::run_token_daemon`/`api::list_containers`/
  `api::stop_container` functions; the adapters are the Clap parse
  structs in `cli` and the `*_cli` wrappers in `main.rs`. Do not invent
  new layers; the existing two-layer split is sufficient.
- `commit-message`: keep commit messages atomic, imperative, and
  scoped to one stage at a time.
- `pr-creation`: open the draft PR for review before implementation
  begins.

## Constraints

- The dual-delivery model in `docs/podbot-design.md` (CLI is the
  adapter; library owns the semantic contract) must be preserved.
- No public library API may accept or return Clap types or
  `clap::Parser`-derived enums. Library types convert into and out of
  the CLI-facing `AgentKindArg` and `AgentModeArg` enums only at the
  `cli` boundary.
- `podbot::cli` must remain behind the existing `cli` feature gate, and
  the library exposes no transitive `clap` types via its stable
  re-exports.
- `podbot host` dispatch must emit zero bytes to stdout from the
  moment Clap completes parsing through library return. All
  operator-facing diagnostics route to stderr (preferably via
  `tracing::warn!` for failure paths and structured `tracing::info!`
  for lifecycle events). Status banners are forbidden on stdout.
  Citation: `docs/developers-guide.md` ("No banners, progress
  indicators, or status messages on stdout") and the MCP stdio
  transport MUST.
- `podbot run` and `podbot exec` dispatch must not regress. Their
  existing behaviour, tests, and documented contract stay intact.
- `podbot ps`, `podbot stop`, and `podbot token-daemon` may continue
  to print operator-facing status banners on stdout because none of
  them ever proxies a protocol stream. The constraint above applies
  only to `host`.
- Unit coverage must use `rstest`. Behavioural coverage must use
  `rstest-bdd` v0.5.0 (already pinned in `Cargo.toml`).
- Tests must cover happy paths, unhappy paths, and relevant edge
  cases. For `host`, the unhappy-path coverage must include the
  semantic rejection of `agent.mode = "podbot"`.
- Documentation must use en-GB-oxendict spelling (the "-ize" / "-yse"
  / "-our" forms).
- When the feature is complete, update only the four relevant roadmap
  checkboxes inside 6.1.1b (`host`, `token-daemon`, `ps`, `stop`). Do
  not mark 6.1.1 itself complete unless every subcommand checkbox
  underneath it is done, and do not mark any 6.2-6.5 step complete.
- Quality gates must run sequentially through `tee` logs because
  concurrent Cargo invocations in this repo can block on build locks.
  Use `/tmp/$ACTION-$(get-project)-$(git branch --show-current).out`
  as the log filename template.
- No new external dependencies may be added. The work is structural;
  it reuses `clap`, `serde`, `tracing`, `rstest`, and `rstest-bdd` as
  already pinned.
- The `experimental` Cargo feature gate continues to mark
  orchestration entry points that are not yet implemented. New
  library functions follow the same gating until the corresponding
  roadmap step lands.
- Module size stays under the 400-line cap defined in
  `AGENTS.md`. Split `src/api/host.rs` if it approaches that ceiling
  during implementation.

## Tolerances (exception triggers)

- Scope: if the implementation exceeds approximately 18 files or
  roughly 1,200 net lines of code across `src/` plus `tests/`, stop
  and reassess whether the plan has absorbed orchestration work that
  belongs to 6.2-6.5.
- Interface: if any change requires modifying a public type already
  documented as stable in `docs/podbot-design.md`
  ("Public library API reference" section) in a breaking way, stop
  and present the proposed change with a migration note before
  proceeding.
- Dependencies: if implementation appears to require a new external
  crate, stop and ask before adding it. The plan deliberately reuses
  existing crates.
- Iterations: if test failures after a code change repeat across more
  than five focused attempts on the same scenario, stop and ask
  before continuing to thrash on the same code path.
- Stdout purity: if any attempt to surface a diagnostic in
  `host_agent_cli` writes to stdout (even transiently while
  debugging), revert and route the diagnostic to stderr before
  re-running tests. Stdout purity is a hard constraint, not a
  tolerance.
- Documentation drift: if implementation reveals that the design
  document, users guide, or roadmap is internally inconsistent in a
  way that requires more than a one-section edit to reconcile, stop
  and capture the inconsistency in `Decision Log` before continuing.
- Ambiguity: if any subcommand reveals two valid request shapes (for
  example, `ListContainersRequest` as a unit type versus a future-
  proof struct), stop and present the trade-off with a recommendation
  rather than implementing both.
- Time: if any single stage (A-G below) takes more than four hours
  of wall-clock work, stop and reassess.

## Risks

- Risk: the existing `host` dispatch hard-codes a `ConfigError`
  return because of issue 51 stdout concerns. Re-enabling that
  dispatch through the library may accidentally restore the
  `println!` banner because the historical CLI helper still uses
  `println!`.
  Severity: medium. Likelihood: medium.
  Mitigation: introduce `host_agent_cli` as a brand-new helper that
  uses only `tracing` macros and explicit `eprintln!` calls for
  diagnostics, delete the dead-coded helper, and add a behavioural
  test that asserts `podbot host` produces zero bytes on stdout when
  the library returns a "not yet implemented" semantic error.

- Risk: `ListContainersRequest` would carry no fields today. Defining
  it as a struct with no fields adds friction without value; defining
  it as a unit type removes future extensibility (filters, formats,
  pagination) and inconsistency with the other request types.
  Severity: low. Likelihood: high.
  Mitigation: ship `ListContainersRequest` as an empty struct with
  `Default`. Document an explicit extensibility note in its
  Rustdoc. Add a parameterized constructor unit test that exercises
  the `Default` path. Revisit only if the 6.3.1 work needs filters
  on day one.

- Risk: semantic validation drift. Today `AppConfig::
  normalize_and_validate(CommandIntent::Run)` rejects hosted modes;
  similar rules will need to apply at the `HostRequest`/`AppConfig`
  validation boundary so `host` rejects `agent.mode = "podbot"`.
  Severity: medium. Likelihood: high.
  Mitigation: re-use the existing
  `AppConfig::normalize_and_validate(CommandIntent::Host)` path
  rather than adding a parallel guard inside `host_agent`. Add a
  `rstest` case that covers the rejection.

- Risk: the experimental gate could trap library consumers. If a
  user compiles `podbot` without `feature = "experimental"`, the
  library should still expose the request types so embedders can
  construct them, even though calling `host_agent`/`run_token_daemon`/
  `list_containers`/`stop_container` returns the documented
  feature-gate error.
  Severity: medium. Likelihood: high.
  Mitigation: gate only the orchestration functions, not the request
  types or their constructors. Add a non-`experimental` library
  embedding test that constructs each request and asserts the type
  is available without the gate.

- Risk: behavioural coverage tries to replace, rather than extend,
  the existing CLI BDD harness. The 6.1.1 plan's mitigation applies
  here too.
  Severity: low. Likelihood: medium.
  Mitigation: extend `tests/features/cli.feature` and
  `tests/features/library_boundary.feature` rather than introducing
  parallel feature files. Add scenarios alongside the existing ones,
  keeping the same `CliState` and `LibraryBoundaryState` fixtures.

- Risk: a future ACP/Codex App Server change introduces a stdout
  byte that the `host` dispatch path must still suppress, and the
  zero-bytes assertion masks it.
  Severity: low. Likelihood: low.
  Mitigation: the zero-bytes assertion runs only at dispatch time
  (before the library hands control to protocol orchestration in
  6.5.1). When 6.5.1 lands it will introduce its own stdout-purity
  coverage for the steady-state protocol path. The two layers
  remain orthogonal.

- Risk: `make fmt` may rewrite Markdown tables introduced by this
  plan into shapes that `markdownlint-cli2` rejects.
  Severity: low. Likelihood: medium.
  Mitigation: follow the documentation conventions established by the
  6.1.1 plan (prose-style descriptions over wide tables when
  continuation rows would be needed; otherwise compact tables). Run
  `make fmt`, then `make markdownlint` early to catch this before
  the test gate.

## Stages

### Stage A: orient and propose (no code changes)

Re-read the reference map and confirm the implementation plan below
still matches the codebase. Compare current `src/main.rs` and
`src/cli/mod.rs` against the expected dispatch shape. Cross-check
`docs/podbot-design.md` and `docs/users-guide.md` for any drift since
the plan was approved. If anything has moved, update the relevant
sections of this plan before proceeding to Stage B. End Stage A by
ticking the corresponding `Progress` entry and by recording any
discovered drift in `Surprises & Discoveries`.

### Stage B: introduce library-owned request types and library functions

Add the four new request modules under `src/api/` (`host.rs`,
`token_daemon.rs`, `stop.rs`, and `list_containers.rs`). Each module
defines a public struct with private fields and accessor methods, a
constructor that validates its inputs through
`ConfigError::MissingRequired` or `ConfigError::InvalidValue`, and
the customary Rustdoc example that compiles. Re-export each type from
`src/api/mod.rs` alongside `RunRequest` and `ExecRequest`.

For each command, add the matching library function inside
`src/api/mod.rs` (or in the new module file if doing so keeps the
file under the size cap):

- `host_agent(config: &AppConfig, request: &HostRequest) ->
  PodbotResult<CommandOutcome>` (experimental). It validates the
  request against the configured `agent.mode`, returns a documented
  semantic error if the validated mode is `Podbot`, and otherwise
  returns the new "not yet implemented" error variant (or reuses
  `ConfigError::InvalidValue` with an actionable reason) until the
  protocol orchestration arrives.
- `run_token_daemon(request: &TokenDaemonRequest) ->
  PodbotResult<CommandOutcome>` (experimental) replaces the existing
  `run_token_daemon(_: &str)` stub. Document the signature change in
  the migration note in `Decision Log`.
- `list_containers(request: &ListContainersRequest) ->
  PodbotResult<CommandOutcome>` (experimental) replaces the existing
  zero-argument stub.
- `stop_container(request: &StopRequest) -> PodbotResult<CommandOutcome>`
  (experimental) replaces the existing `stop_container(_: &str)` stub.

Validation rules at the request layer:

- `HostRequest::new(agent_kind, agent_mode)` accepts `Option`
  overrides matching the CLI shape, normalizes them into the
  library-owned `AgentKind` and `AgentMode` types, and rejects the
  `Podbot` mode immediately so embedders cannot construct an
  obviously-illegal hosted request.
- `TokenDaemonRequest::new(container_id)` rejects blank and
  whitespace-only identifiers via `ConfigError::MissingRequired
  { field: "token-daemon.container_id" }` (or the closest existing
  variant).
- `StopRequest::new(container)` rejects blank and whitespace-only
  identifiers in the same way.
- `ListContainersRequest::default()` returns an empty struct.

Recommended file targets in this stage:

- `src/api/mod.rs`
- `src/api/host.rs`
- `src/api/token_daemon.rs`
- `src/api/stop.rs`
- `src/api/list_containers.rs`
- `src/api/tests.rs` (extend, do not replace)
- `src/error.rs` only if a new semantic variant is genuinely needed.
  Default to reusing existing variants.

Stage B ends with the new types and functions compiling under both
the default feature set and `--all-features`. The CLI dispatch still
calls the old signatures at this stage; rewiring happens in Stage C.

### Stage C: rewire the CLI adapter around the new request boundary

Update `src/cli/mod.rs` so each of `HostArgs`, `TokenDaemonArgs`, and
`StopArgs` exposes a `to_*Request()` method that builds the
corresponding library type. `Commands::Ps` builds a
`ListContainersRequest::default()` inline at the dispatch site (no
flag data exists today).

Update `src/main.rs::run` so:

- `Commands::Run(args)` continues to route through `RunRequest` and
  `run_agent_cli` (no change).
- `Commands::Host(args)` builds `HostRequest`, then calls a new
  `host_agent_cli` that emits zero bytes to stdout. All diagnostics
  go through `tracing::info!` / `tracing::warn!` / `eprintln!`. The
  current `#[expect(dead_code)]` helper is deleted. The hard-coded
  `ConfigError::InvalidValue` short-circuit goes away because the
  rejection is now expressed through `host_agent`'s library-side
  semantic error.
- `Commands::TokenDaemon(args)` builds `TokenDaemonRequest` and calls
  the updated `run_token_daemon_cli`. Its stdout banner is preserved
  because `token-daemon` does not proxy a protocol stream.
- `Commands::Ps` builds `ListContainersRequest::default()` and calls
  the updated `list_containers_cli`. Its stdout banner stays.
- `Commands::Stop(args)` builds `StopRequest` and calls the updated
  `stop_container_cli`. Its stdout banner stays.
- `Commands::Exec(args)` is unchanged.

Resolve issue 51 explicitly: when `host_agent_cli` lands, remove the
`FIXME(https://github.com/leynos/podbot/issues/51)` markers on the
`api/mod.rs` stubs that this plan replaces with real semantic returns
or with the new request-typed signatures. Record the resolution in
the `Decision Log` and the PR description so the issue can be
closed when 6.1.1b ships.

End Stage C with `cargo check` succeeding under
`--no-default-features`, `--features cli`, and
`--all-features`. Manual smoke check: run `podbot host
--agent codex --agent-mode codex_app_server` and confirm by visual
inspection that stdout is empty before the semantic error.

### Stage D: extend test coverage

Add or update `rstest` coverage for:

- Each new request constructor's happy path (one parameterized case
  per request type via `#[rstest]`/`#[case]`).
- Each new request constructor's unhappy path (blank identifier;
  whitespace-only identifier; `HostRequest` rejecting `Podbot` mode;
  any other validation rule expressed in Stage B).
- Each `*_cli` wrapper's behaviour under the experimental gate:
  - `host_agent_cli` returns the library's semantic error without
    writing to stdout. Capture both stdout and stderr in the test
    via the existing capture helpers; assert stdout is exactly zero
    bytes and stderr contains the expected diagnostic.
  - The other `*_cli` wrappers preserve their existing banner
    behaviour. Their tests should assert the banner appears on
    stdout and the wrapper returns the library's stub success.
- A library embedding case that constructs each new request without
  importing `podbot::cli`, mirroring the existing
  `run_request_can_be_constructed_without_cli_types` and
  `stub_orchestration_functions_return_success` tests in
  `tests/library_embedding.rs`.

Add or update `rstest-bdd` scenarios in
`tests/features/cli.feature`:

- `Host command requires --agent-mode`: invoke
  `podbot host --agent codex` (or whichever flag combination triggers
  the Clap failure given the existing optional defaults) and assert
  Clap reports the missing required flag in stderr.
- `Host command rejects interactive mode`: invoke
  `podbot host --agent codex --agent-mode podbot` and assert the
  library returns a semantic error on stderr and exits non-zero.
- `Host command help documents required arguments`: invoke
  `podbot host --help` and assert stdout mentions
  `--agent-mode`.
- `Host command emits no stdout banner`: invoke
  `podbot host --agent codex --agent-mode codex_app_server`,
  capture combined output, and assert stdout is empty.
- `Stop command requires container`: invoke `podbot stop` with no
  positional argument and assert Clap reports the missing argument.
- `Stop command succeeds with container argument`: extend the
  existing parse-success scenario family to cover `podbot stop
  abc123`.
- `Token-daemon command requires container ID`: invoke
  `podbot token-daemon` without an argument and assert Clap fails.
- `Ps command help documents the command`: invoke
  `podbot ps --help` and assert stdout mentions a description of the
  subcommand.

Add or update `rstest-bdd` scenarios in
`tests/features/library_boundary.feature` so a library consumer can:

- Construct `HostRequest`, `TokenDaemonRequest`,
  `ListContainersRequest`, and `StopRequest` directly.
- Call each library function under the `experimental` feature and
  observe the documented stub outcome (success, or the new "not yet
  implemented" semantic error variant for `host_agent` when the mode
  is hosted).
- Observe semantic validation errors when an illegal value is
  supplied.

Stage D ends with `make test` passing the full workspace under both
the default feature set and `--all-features`. Capture the test log
to `/tmp/test-podbot-$(git branch --show-current).out` so the
implementer can resume after a context switch.

### Stage E: update design, user, and roadmap documentation

Update `docs/podbot-design.md`:

- Extend the "Dual delivery model" section with a sentence noting
  that all six operator subcommands route through library-owned
  request types.
- Extend the "Public library API reference" section to list
  `HostRequest`, `TokenDaemonRequest`, `ListContainersRequest`, and
  `StopRequest` alongside `RunRequest` and `ExecRequest`. Mark them
  as stable types whose constructors validate inputs; mark the
  matching `host_agent`/`run_token_daemon`/`list_containers`/
  `stop_container` functions as experimental orchestration entry
  points.
- Reconcile the `host` description with the new dispatch shape:
  `podbot host` parses through Clap, builds `HostRequest`, and the
  library currently returns a documented "not yet implemented"
  error. Diagnostics route to stderr only.

Update `docs/users-guide.md`:

- Replace the "temporarily unavailable" prose for `host` with the
  new contract: parsing succeeds; the library returns a documented
  error today; the operator never sees status text on stdout.
- Tighten the `token-daemon`, `ps`, and `stop` entries to note that
  each subcommand parses through Clap and dispatches through a
  library-owned request, so a Rust embedder can call the equivalent
  library function directly.
- Add a small "Library embedding" subsection (or extend the
  existing one if present) listing the new request types and the
  rule that operators see banners on `run`, `token-daemon`, `ps`,
  `stop`, and `exec` but never on `host`.

Update `docs/podbot-roadmap.md` only by ticking the four newly
delivered checkboxes inside the 6.1.1b "Implemented scope" list.
Leave 6.2-6.5 untouched and leave the 6.1.1 parent entry only as
done if and only if every sub-checkbox is ticked.

If any decision in this plan is significant enough to warrant an
ADR (for example, "library-owned request types for every operator
subcommand" as a project-wide convention), draft a short ADR under
`docs/` following the documentation style guide and reference it
from the design document. Default to recording the decision inline
in the design document rather than as a standalone ADR; reach for
an ADR only if the decision is contentious or has cross-cutting
consequences.

End Stage E by running `make markdownlint` and `make nixie` to
validate the Markdown edits, and by capturing the logs to
`/tmp/markdownlint-podbot-$(git branch --show-current).out` and
`/tmp/nixie-podbot-$(git branch --show-current).out`.

### Stage F: full validation sequence

Run the validation sequence sequentially with `tee` logs:

```bash
set -o pipefail && make fmt 2>&1 | tee /tmp/fmt-podbot-$(git branch --show-current).out
set -o pipefail && make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-$(git branch --show-current).out
set -o pipefail \
  && MDLINT=/home/leynos/.bun/bin/markdownlint-cli2 make markdownlint 2>&1 \
  | tee /tmp/markdownlint-podbot-$(git branch --show-current).out
set -o pipefail && make nixie 2>&1 | tee /tmp/nixie-podbot-$(git branch --show-current).out
set -o pipefail && make lint 2>&1 | tee /tmp/lint-podbot-$(git branch --show-current).out
set -o pipefail && make test 2>&1 | tee /tmp/test-podbot-$(git branch --show-current).out
```

Expected outcomes:

- formatting and Markdown validation pass without manual edits,
- clippy and docs linting pass with no warnings,
- the full workspace test suite passes, including the new request
  and CLI scenarios.

If any gate fails, fix the underlying issue and re-run from the
failing gate onwards. Do not request a CodeRabbit review until every
gate above succeeds.

### Stage G: CodeRabbit review and iteration

Run `coderabbit review --agent` against the branch. Address every
concern raised, then re-run the validation gates in Stage F. Do not
move to the next major milestone (PR finalization) until CodeRabbit
returns no remaining concerns.

If CodeRabbit raises a concern that contradicts this plan's
constraints (for example, a suggestion to allow stdout in the
`host` dispatch path), record the disagreement in `Decision Log`
with the rationale for keeping the constraint, and reply on the
review with the same reasoning.

## Interfaces and dependencies

This plan adds the following public types and functions to
`podbot::api`. All types live outside `feature = "experimental"`; the
listed orchestration functions remain gated until the corresponding
roadmap step lands.

In `src/api/host.rs`:

```rust
/// Request to host a long-lived agent protocol session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRequest { /* private fields */ }

impl HostRequest {
    pub fn new(
        agent_kind: Option<AgentKind>,
        agent_mode: Option<AgentMode>,
    ) -> PodbotResult<Self>;

    pub fn agent_kind(&self) -> Option<AgentKind>;
    pub fn agent_mode(&self) -> Option<AgentMode>;
}
```

In `src/api/token_daemon.rs`:

```rust
/// Request to start the token refresh daemon for a sandbox container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenDaemonRequest { /* private fields */ }

impl TokenDaemonRequest {
    pub fn new(container_id: impl Into<String>) -> PodbotResult<Self>;
    pub fn container_id(&self) -> &str;
}
```

In `src/api/list_containers.rs`:

```rust
/// Request to list running podbot containers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListContainersRequest;
```

In `src/api/stop.rs`:

```rust
/// Request to stop a running podbot container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopRequest { /* private fields */ }

impl StopRequest {
    pub fn new(container: impl Into<String>) -> PodbotResult<Self>;
    pub fn container(&self) -> &str;
}
```

In `src/api/mod.rs` under `feature = "experimental"`:

```rust
pub fn host_agent(config: &AppConfig, request: &HostRequest)
    -> PodbotResult<CommandOutcome>;
pub fn run_token_daemon(request: &TokenDaemonRequest)
    -> PodbotResult<CommandOutcome>;
pub fn list_containers(request: &ListContainersRequest)
    -> PodbotResult<CommandOutcome>;
pub fn stop_container(request: &StopRequest)
    -> PodbotResult<CommandOutcome>;
```

The CLI adapter side gains:

```rust
impl HostArgs    { pub fn to_host_request(&self) -> PodbotResult<HostRequest>; }
impl TokenDaemonArgs { pub fn to_token_daemon_request(&self) -> PodbotResult<TokenDaemonRequest>; }
impl StopArgs    { pub fn to_stop_request(&self) -> PodbotResult<StopRequest>; }
// Commands::Ps builds ListContainersRequest::default() at the dispatch site.
```

No new external crates are required. No existing public type changes
shape; the changes are additive. The previously published
`run_token_daemon(_: &str)`, `list_containers()`, and
`stop_container(_: &str)` signatures are experimental and may
change. Record this signature change explicitly in `Decision Log`
so future readers can see why the migration was acceptable.

## Validation and acceptance

Quality criteria — "done" means:

- Tests: all `rstest`, `rstest-bdd`, and library-boundary tests pass
  under both `cargo test --workspace` and
  `cargo test --workspace --all-features`. New scenarios cover happy
  paths, unhappy paths, the `host` stdout-purity assertion, and the
  semantic rejection of `agent.mode = "podbot"` for `HostRequest`.
- Lint and format: `make check-fmt`, `make lint`, `make markdownlint`,
  and `make nixie` all pass cleanly.
- CodeRabbit: `coderabbit review --agent` returns no remaining
  concerns after the final iteration.
- Documentation: `docs/podbot-design.md`, `docs/users-guide.md`, and
  `docs/podbot-roadmap.md` reflect the new boundary, and no other
  document drifts.
- Behavioural acceptance, captured by the new tests and by manual
  observation:
  - `podbot host --agent codex --agent-mode codex_app_server`
    prints nothing on stdout, emits a documented "not yet
    implemented" error on stderr, and exits non-zero.
  - `podbot host --agent codex --agent-mode podbot` fails with a
    semantic error mentioning that `run` is interactive-only, again
    with stdout empty.
  - `podbot token-daemon abc123` prints the same operator-facing
    status banner it does today and routes through
    `TokenDaemonRequest`.
  - `podbot ps` prints its placeholder listing and routes through
    `ListContainersRequest`.
  - `podbot stop my-container` prints its placeholder status and
    routes through `StopRequest`.
  - `podbot exec my-container -- echo hello` is unchanged.

Quality method (how we check):

- Run the Stage F validation sequence in order.
- Run `cargo test --workspace --features experimental` to cover the
  experimental orchestration paths explicitly.
- Manually exercise each subcommand once, capturing combined output
  via `2>&1 | tee` and inspecting the stdout/stderr split for
  `host`.

## Idempotence and recovery

All Stage A-E edits are file edits and may be re-run safely. Stage F
gates produce log files under `/tmp` that can be inspected and
discarded between runs. If `cargo build` or `cargo test` enters a
cache-related failure mode, run `cargo clean -p podbot` once and
retry; if the inconsistency persists, stop and investigate before
continuing. Do not nuke the shared Cargo cache.

If the branch needs to be rebased onto `origin/main` mid-flight, run
`git fetch origin && git rebase origin/main`, then re-run Stage F.
Capture any rebase-induced changes in `Surprises & Discoveries` and
in the `Decision Log` so future readers can see why the diff grew.

## Artifacts and notes

Capture the final log artifacts in `/tmp` and reference them in the
PR description. At minimum:

- `/tmp/check-fmt-podbot-$(git branch --show-current).out`
- `/tmp/lint-podbot-$(git branch --show-current).out`
- `/tmp/test-podbot-$(git branch --show-current).out`
- `/tmp/markdownlint-podbot-$(git branch --show-current).out`
- `/tmp/nixie-podbot-$(git branch --show-current).out`

If CodeRabbit raises and resolves concerns, save the final review
transcript alongside these logs.

## Progress

Progress entries are appended as each stage completes. Each entry
should carry a UTC timestamp (`YYYY-MM-DD HH:MMZ`) so the
implementation team can measure rates of progress.

- [x] (2026-05-27 13:00Z) Reviewed `docs/podbot-roadmap.md`,
  `docs/podbot-design.md`, `docs/users-guide.md`,
  `docs/execplans/6-1-1-run-subcommand.md`,
  `src/cli/mod.rs`, `src/main.rs`, `src/api/`, and the existing CLI
  and library-boundary tests. Confirmed that `run` and `exec` already
  route through library-owned request types and that the remaining
  four subcommands have library stubs but no library-owned request
  types.
- [x] (2026-05-27 13:00Z) Drafted this ExecPlan for approval.
- [ ] Stage A: orient and confirm the plan still matches the
  codebase.
- [ ] Stage B: introduce library-owned request types and library
  functions.
- [ ] Stage C: rewire the CLI adapter around the new request
  boundary.
- [ ] Stage D: extend `rstest` and `rstest-bdd` coverage.
- [ ] Stage E: update design, user, and roadmap documentation.
- [ ] Stage F: run the full validation sequence.
- [ ] Stage G: CodeRabbit review and iteration until clean.

## Surprises & discoveries

- Observation: the current `Commands::Host` arm in `src/main.rs`
  hard-codes a `ConfigError::InvalidValue` return with the message
  "the host subcommand is temporarily disabled until host_agent_cli
  writes diagnostics to stderr only". The dead-coded helper still
  uses `println!`.
  Evidence: `src/main.rs:66-76` and `src/main.rs:182-196`.
  Impact: re-enabling the dispatch path is part of the scope, not
  blocked work. Issue 51 can close when 6.1.1b ships.

- Observation: `ListContainersRequest` would have no fields today.
  Evidence: the existing `list_containers()` stub takes no
  arguments.
  Impact: shipping it as an empty struct keeps the request boundary
  symmetric with the other request types and lets future filtering
  or pagination flags grow without a breaking change.

- Observation: `AgentKindArg` and `AgentModeArg` already convert into
  the library `AgentKind` and `AgentMode` enums via `From`
  implementations in `src/cli/mod.rs`.
  Evidence: `src/cli/mod.rs:30-61`.
  Impact: `HostArgs::to_host_request()` can reuse those conversions
  rather than introducing parallel mappings.

- Observation: `tests/features/library_boundary.feature` already has
  a "Stub orchestration functions return success" scenario that
  exercises the experimental stubs.
  Evidence: `tests/features/library_boundary.feature:25-29` and
  `tests/library_embedding.rs:215-237`.
  Impact: the existing scenario must be extended to construct the
  new request types and to surface the new semantic-error variants
  rather than being replaced.

(Append further entries here as implementation proceeds.)

## Decision log

- Decision: shape each remaining subcommand's library boundary as a
  dedicated request type (`HostRequest`, `TokenDaemonRequest`,
  `ListContainersRequest`, `StopRequest`) rather than continuing to
  pass raw `&str` values to library stubs.
  Rationale: matches the established pattern set by `RunRequest` and
  `ExecRequest`; lets embedders construct each operation without
  importing `podbot::cli`; cited by Jujutsu's "Separation of
  library from UI" architecture rule and Alexis Lozano's hexagonal
  CLI Rust example.
  Date/Author: 2026-05-27 / Claude (planning).

- Decision: keep `ListContainersRequest` as an empty struct with
  `Default` rather than skipping the request type or making it a
  unit struct.
  Rationale: symmetry with the other request types; future flags
  such as filters, pagination, or output format can be added
  without a breaking change; documented extensibility note in
  Rustdoc.
  Date/Author: 2026-05-27 / Claude (planning).

- Decision: gate the new orchestration functions on
  `feature = "experimental"` (consistent with the existing
  `run_agent`, `run_token_daemon`, `list_containers`, and
  `stop_container` stubs) but keep the new request types stable and
  unconditionally available.
  Rationale: embedders need a stable surface to construct requests
  at compile time even when the orchestration is not yet wired up.
  Date/Author: 2026-05-27 / Claude (planning).

- Decision: enforce stdout purity for `podbot host` at the
  dispatch layer (not just inside future protocol orchestration).
  Rationale: the MCP stdio transport specification is unambiguous
  that the server MUST NOT write anything to stdout that is not a
  valid protocol message; once 6.5.1 lands, any pre-existing
  banner would corrupt the very first protocol byte. Asserting
  zero-bytes-on-stdout at dispatch time prevents regressions
  before the protocol orchestration is on the hot path.
  Date/Author: 2026-05-27 / Claude (planning).

- Decision: replace the previously published experimental
  signatures `run_token_daemon(_: &str)`, `list_containers()`, and
  `stop_container(_: &str)` with request-typed versions even
  though they were technically already part of the experimental
  surface.
  Rationale: the experimental gate exists exactly so that
  pre-stabilization shape changes are permissible; making them
  request-typed now avoids a second migration when the real
  orchestration arrives.
  Date/Author: 2026-05-27 / Claude (planning).

- Decision: defer the operator override for ACP host-side
  delegation (roadmap 2.6.3) to its own ExecPlan, even though
  `HostRequest` naturally has an opinion about ACP modes.
  Rationale: 2.6.3 is a configuration/validation concern that
  affects both `run` and `host`, and folding it into 6.1.1b would
  inflate scope past the tolerance ceiling.
  Date/Author: 2026-05-27 / Claude (planning).

(Append further decisions here as implementation proceeds.)

## Outcomes & retrospective

To be completed at the end of implementation. Summarize what was
delivered, what changed from the plan, and what would be done
differently next time. Compare the result against the purpose stated
above. Note any debt this work leaves for future roadmap steps
(specifically 6.2-6.5).

## Revision note

(2026-05-27) Initial draft.
