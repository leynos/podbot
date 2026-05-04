# Step 3.2.1: Acquire installation token with expiry buffer

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose and big picture

Enable podbot to exchange a configured GitHub App identity for an
installation-scoped access token that is safe to use for repository operations.
After this change, the GitHub integration will expose a single high-level
helper named `installation_token_with_buffer` that:

- loads the configured RSA private key,
- builds an authenticated App client,
- acquires an installation token for `github.installation_id`,
- rejects tokens that are already expired or too close to expiry for the
  requested buffer,
- returns the token string together with expiry metadata for later refresh
  work, and
- maps GitHub or transport failures into semantic `GitHubError` variants
  rather than opaque reports.

This step is intentionally narrower than the later token-daemon work. It does
not create runtime directories, write token files, refresh tokens in a loop, or
mount secrets into the container. Those behaviours belong to roadmap Step 3.3
and Step 3.4.

Observable outcome: running `make test` passes and new coverage exists in both
unit tests and Behaviour-Driven Development (BDD) scenarios for the following
cases:

- a valid installation token whose expiry is beyond the buffer succeeds,
- a token whose expiry falls inside the buffer fails deterministically,
- GitHub API rejection is surfaced as `TokenAcquisitionFailed`,
- malformed or missing expiry metadata fails closed with an actionable error,
  and
- the returned value exposes the token string plus expiry metadata that later
  steps can consume without scraping logs.

This request also includes completion criteria about interactive terminals,
resize propagation, and exit-code forwarding. Those criteria belong to roadmap
Step 2.4, not Step 3.2. This plan therefore treats the Step 3.2 roadmap entry
in `docs/podbot-roadmap.md` as the authoritative completion target for this
work.

## Constraints

- Keep this change scoped to installation-token acquisition. Do not implement
  the Step 3.3 token daemon, token file writes, runtime-directory creation,
  `GIT_ASKPASS`, or workspace cloning in this step.
- Preserve the current dual-delivery boundary: library code returns data and
  semantic errors; CLI adapters own printing, formatting, and process exit.
- Use the existing semantic error surface in `src/error.rs`. Installation-token
  failures must land in `GitHubError::TokenAcquisitionFailed` or
  `GitHubError::TokenExpired`, not `eyre::Report` or ad hoc strings at the
  library boundary.
- Keep files under 400 lines. Current budgets at planning time:
  `src/github/mod.rs` is 319 lines and `src/github/tests.rs` is 398 lines, so
  new token-acquisition tests must not be added to `src/github/tests.rs`.
- Maintain `missing_docs = "deny"` and the existing Rustdoc quality bar for
  any new public items.
- Use en-GB-oxendict spelling in documentation.
- Use `rstest` for unit coverage and `rstest-bdd` v0.5.0 for behavioural
  coverage.
- Keep tests deterministic. Do not add hard dependencies on a live GitHub
  account, real network access, or mutable global environment state.
- Respect the testing guidance in
  `docs/reliable-testing-in-rust-via-dependency-injection.md`: do not read the
  wall clock directly in logic that needs deterministic buffer checks.
- Do not add a new general-purpose logging dependency merely to emit token
  expiry text. If expiry logging is required, preserve expiry metadata in the
  returned value and emit it only from an existing adapter layer or a narrowly
  justified logging seam.
- Do not change roadmap status to done until the implementation and all quality
  gates have passed.

## Tolerances (exception triggers)

- Scope: if the implementation grows beyond 14 files or 350 net new lines,
  stop and confirm scope before proceeding.
- API shape: if preserving expiry metadata requires a broader public API than a
  single token value object plus helper functions, stop and confirm before
  widening the surface.
- Dependencies: if the chosen approach requires more than two new direct
  dependencies, stop and confirm. The expected upper bound is one direct
  `chrono` dependency, plus a possible `mockable` feature change.
- Logging: if the only way to satisfy the expiry-debugging requirement is to
  introduce a new logging framework or print from library code, stop and
  confirm. That would be a policy decision, not an implementation detail.
- Library coupling: if extending `GitHubAppClient` forces more than four
  unrelated test harnesses to change, stop and compare that cost against
  introducing a dedicated token-acquisition seam.
- Iterations: if `make lint` or `make test` still fails after three targeted
  fix passes, stop and record blocker evidence in this plan before changing the
  design.

## Risks

- Risk: Octocrab's public convenience method
  `installation_token_with_buffer(chrono::Duration)` returns a `SecretString`
  but does not surface expiry metadata, while the roadmap asks podbot to log or
  otherwise preserve expiry time. Severity: high. Likelihood: certain.
  Mitigation: start with a short implementation spike that compares two
  approaches:
  1. use Octocrab's convenience method directly and accept that expiry must be
  recovered some other way, or
  2. use a typed installation-token response path that preserves `token` and
  `expires_at`, then apply podbot's own buffer check with an injected clock.
  The preferred path in this plan is option 2.

- Risk: a literal call to Octocrab's convenience method needs
  `chrono::Duration`, but the crate does not currently declare `chrono` as a
  direct dependency. Severity: medium. Likelihood: high. Mitigation: if the
  final implementation needs `chrono`, add it as a direct dependency using a
  implicit semver version that matches the currently locked `chrono` version
  family and the project's Cargo.toml guidance.

- Risk: `src/github/tests.rs` is already at 398 lines. Adding new tests there
  will violate the repository's 400-line rule. Severity: high. Likelihood:
  certain. Mitigation: create a new test submodule such as
  `src/github/installation_token_tests.rs`.

- Risk: `build_app_client` is synchronous in shape but still requires an active
  Tokio runtime because Octocrab spawns a Tower buffer task internally.
  Severity: medium. Likelihood: certain. Mitigation: every production entry
  point and every test that exercises real `Octocrab` construction must create
  and enter a runtime explicitly.

- Risk: `run_agent_cli` currently validates only `app_id` and
  `private_key_path`, so `installation_id` is not yet enforced on the path that
  will eventually need repository tokens. Severity: medium. Likelihood: high.
  Mitigation: keep Step 3.2 scoped to the GitHub helper itself, but record in
  the plan exactly where later orchestration work must switch from partial
  field checks to `GitHubConfig::validate()`.

- Risk: the repository does not currently have a shared logging substrate.
  Adding ad hoc stderr printing inside `src/github` would violate the library
  boundary. Severity: medium. Likelihood: high. Mitigation: treat preserved
  expiry metadata as the core deliverable in Step 3.2, and keep actual
  operator-facing logging at the adapter layer when the acquisition helper is
  wired into orchestration.

## Progress

- [x] 2026-04-20 UTC: Draft ExecPlan created.
- [x] 2026-04-20 UTC: Surveyed `docs/podbot-roadmap.md`,
  `docs/podbot-design.md`, `docs/users-guide.md`, and the Rust testing guides.
- [x] 2026-04-20 UTC: Surveyed the current GitHub auth code, test seams, and
  Octocrab source for `installation_token_with_buffer`.
- [x] 2026-04-22 UTC: Confirmed the acquisition seam. Podbot now calls
  Octocrab's typed `POST /app/installations/{installation_id}/access_tokens`
  path instead of the convenience helper so expiry metadata is preserved.
- [x] 2026-04-22 UTC: Added `src/github/installation_token.rs` with
  `InstallationAccessToken`, `installation_token_with_buffer`, and the injected
  `installation_token_with_factory` test seam.
- [x] 2026-04-22 UTC: Added focused unit tests in
  `src/github/installation_token_tests.rs` for expiry validation, metadata
  failures, debug redaction, and client error propagation.
- [x] 2026-04-22 UTC: Added behavioural coverage in
  `tests/bdd_github_installation_token.rs` and
  `tests/features/github_installation_token.feature`.
- [x] 2026-04-22 UTC: Updated `docs/podbot-design.md`,
  `docs/users-guide.md`, and `docs/podbot-roadmap.md` to reflect the
  implemented Step 3.2 contract.
- [x] 2026-04-22 UTC: Marked the Step 3.2 roadmap task as done.
- [x] 2026-04-22 UTC: Ran documentation validation. `make markdownlint` and
  `make nixie` passed. `make fmt` still fails because the pre-existing
  `mdtablefix` / markdownlint table breakage in `docs/podbot-design.md` and
  `docs/execplans/5-3-1-stabilize-public-library-boundaries.md` remains
  unresolved.
- [x] 2026-04-22 UTC: Ran `make check-fmt`, `make lint`, and `make test`
  successfully after the implementation and test fixes landed.
- [x] 2026-04-22 UTC: Finalized `Surprises & Discoveries`, `Decision Log`,
  and `Outcomes & Retrospective`.

## Surprises & Discoveries

- Octocrab v0.49.5 exposes two relevant public surfaces:
  `Octocrab::installation(InstallationId)` and
  `Octocrab::installation_token_with_buffer(chrono::Duration)`. The latter
  returns only a token secret and keeps expiry inside Octocrab's internal cache
  rather than exposing it to podbot.

- Octocrab also ships a public `octocrab::models::InstallationToken` type with
  `token: String` and `expires_at: Option<String>`. That makes a typed,
  podbot-owned response path feasible if the implementation chooses to preserve
  expiry explicitly instead of relying on Octocrab's cached-token helper.

- `src/github/tests.rs` is already effectively full. New token-acquisition unit
  tests need a dedicated file from the outset.

- Existing GitHub BDD harnesses already demonstrate the preferred testing
  pattern for this subsystem: a `mockall::mock!` seam in the BDD helper
  directory, `ScenarioState` with `Slot<T>`, and one scenario file per
  behaviour slice.

- Octocrab's generic `post` method is sufficient for the installation-token
  path, but the generic parameter order is `post::<Body, Response>`, not the
  more common response-first pattern. The implementation now uses
  `post::<_, InstallationToken>(...)`.

- Octocrab marks `InstallationToken` as `#[non_exhaustive]`, so tests cannot
  build it with a struct literal. Unit and BDD tests instead deserialize a
  minimal JSON payload into `InstallationToken`.

- `make fmt` still is not idempotent for the repository’s existing Markdown
  tables. Re-running it on this branch reproduces the previously observed MD056
  and MD060 failures in `docs/podbot-design.md` and
  `docs/execplans/5-3-1-stabilize-public-library-boundaries.md`, even though
  `make markdownlint` passes once those files are restored to their checked-in
  table layout.

## Decision Log

- 2026-04-20 UTC: Treat the Step 3.2 roadmap entry as authoritative for this
  plan. The completion text about interactive terminal handling in the request
  belongs to Step 2.4 and does not redefine this GitHub work item.

- 2026-04-20 UTC: Prefer a podbot-owned token value object over returning a raw
  `String`. The value object should expose the token string for Git operations
  while also preserving expiry metadata for Step 3.3 and redacting secrets in
  `Debug`.

- 2026-04-20 UTC: Prefer a dedicated token-acquisition seam rather than
  overloading the existing `validate_with_factory` helper. Credential
  validation and installation-token acquisition are adjacent but not identical
  behaviours, and separate helpers keep the code and tests easier to read.

- 2026-04-22 UTC: Implement the production path with a podbot-owned
  `InstallationAccessToken` value object and an injected
  `GitHubInstallationTokenClient` seam. This keeps the public helper narrow,
  preserves expiry metadata, and lets tests exercise the orchestration path
  without live network access.

- 2026-04-22 UTC: Use `chrono` as a direct dependency for RFC 3339 parsing and
  UTC buffer comparison. The standard library alone would have required either
  a larger bespoke parser or weaker expiry handling.

- 2026-04-22 UTC: Keep the public production helper signature at the
  four-argument shape requested by the roadmap, but collapse the injected test
  seam into `InstallationTokenRequest` so Clippy’s argument-count rule is
  satisfied without suppressions.

## Outcomes & Retrospective

Completed on 2026-04-22 UTC.

Step 3.2 now ships a real installation-token acquisition path in
`src/github/installation_token.rs`. The public helper
`installation_token_with_buffer(app_id, installation_id, private_key_path, buffer)`
 loads the RSA private key, builds an App-authenticated Octocrab client,
requests `POST /app/installations/{installation_id}/access_tokens`, parses
`expires_at`, enforces the expiry buffer, and returns a podbot-owned
`InstallationAccessToken` value object. Tokens that expire within the buffer
raise `GitHubError::TokenExpired`; malformed or missing expiry metadata fails
closed with `GitHubError::TokenAcquisitionFailed`.

The implementation preserved the existing library boundary. Error mapping stays
semantic in `src/github/classify.rs`, secrets are redacted in `Debug`, and the
test seam is explicit via `GitHubInstallationTokenClient` plus
`installation_token_with_factory(InstallationTokenRequest, factory)`. Focused
unit tests and a new BDD feature now cover the success case, within-buffer
expiry rejection, GitHub API rejection, and missing expiry metadata.

Validation evidence:

- `set -o pipefail && make check-fmt 2>&1 | tee /tmp/3-2-1-check-fmt.log`
  exited 0.
- `set -o pipefail && make lint 2>&1 | tee /tmp/3-2-1-lint.log`
  exited 0.
- `set -o pipefail && make test 2>&1 | tee /tmp/3-2-1-test.log`
  exited 0.
- `set -o pipefail && MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint
  2>&1 | tee /tmp/3-2-1-markdownlint.log` exited 0.
- `set -o pipefail && make nixie 2>&1 | tee /tmp/3-2-1-nixie.log`
  exited 0.
- `set -o pipefail && make fmt 2>&1 | tee /tmp/3-2-1-fmt.log`
  still exits non-zero because of the pre-existing table-formatting bug noted
  above; no new formatter issue was introduced by this step.

## Context and orientation

The current GitHub integration lives in `src/github/mod.rs`. It already
provides the Step 3.1 building blocks:

- `load_private_key(key_path)` loads and validates a PEM-encoded RSA key.
- `build_app_client(app_id, private_key)` constructs an App-authenticated
  `Octocrab`.
- `GitHubAppClient` and `OctocrabAppClient` abstract `/app` validation.
- `validate_app_credentials(app_id, private_key_path)` performs startup
  credential validation.

The configuration values needed for Step 3.2 already exist in
`src/config/types.rs` as `GitHubConfig.app_id`, `GitHubConfig.installation_id`,
and `GitHubConfig.private_key_path`.

The current orchestration is still mostly stubbed. `run_agent_cli` in
`src/main.rs` validates App credentials up front but does not yet request an
installation token. `run_token_daemon_cli` calls a stubbed library entry point.
That means Step 3.2 can add a tested helper without yet wiring the full clone
or refresh flow.

The error surface is ready for this step. `src/error.rs` already includes:

- `GitHubError::TokenAcquisitionFailed { message: String }`
- `GitHubError::TokenExpired`
- `GitHubError::TokenRefreshFailed { message: String }`

Only the first two belong to Step 3.2. `TokenRefreshFailed` is for Step 3.3.

The codebase already contains a classifier in `src/github/classify.rs` for
mapping GitHub API failures into actionable authentication messages. Step 3.2
should extend that pattern for installation-token acquisition instead of
creating a second, ad hoc error style.

The key implementation constraint comes from Octocrab itself. The source in the
local Cargo registry shows:

```rust
pub async fn installation_token_with_buffer(
    &self,
    buffer: chrono::Duration,
) -> Result<SecretString>
```

and a public response model:

```rust
pub struct InstallationToken {
    pub token: String,
    pub expires_at: Option<String>,
    // ...
}
```

That mismatch is the main design decision for this step. Podbot must preserve
expiry metadata for the roadmap, but Octocrab's convenience helper returns only
the secret.

## Documentation signposts

The implementing agent should keep the following repository documents open
while working through this plan:

- `docs/podbot-roadmap.md`
  - the authoritative scope and completion criteria for Step 3.2 and the
    boundary with Step 3.3 and Step 3.4.
- `docs/podbot-design.md`
  - the token-management design, the security boundary around host-side token
    handling, and the expectation that refresh work follows later.
- `docs/users-guide.md`
  - the place where user-visible configuration requirements and token-related
    troubleshooting must be updated once implementation lands.
- `docs/rust-testing-with-rstest-fixtures.md`
  - the preferred `rstest` fixture and parameterization patterns for unit
    coverage.
- `docs/rstest-bdd-users-guide.md`
  - the required structure for BDD scenarios, helper modules, and synchronous
    step definitions.
- `docs/reliable-testing-in-rust-via-dependency-injection.md`
  - the testing rule that clock and environment dependencies should be
    injected rather than read directly.
- `docs/rust-doctest-dry-guide.md`
  - guidance for any new public Rustdoc examples on the helper or token value
    object.
- `docs/complexity-antipatterns-and-refactoring-strategies.md`
  - the repository’s guidance for keeping token acquisition, expiry policy, and
    error mapping out of one large “bumpy road” function.
- `docs/ortho-config-users-guide.md`
  - the reference for layered configuration precedence and merge behaviour if
    this work touches how `GitHubConfig` is consumed or validated.

The implementing agent should also refer back to
`docs/execplans/3-1-3-validate-credentials-on-startup.md` and
`docs/execplans/3-1-4-handle-invalid-or-expired-app-credentials-with-clear-errors.md`
 because Step 3.2 builds directly on those seams and should preserve their
error quality bar.

## Skills to load

Before implementation starts, the agent should load the smallest relevant skill
set rather than freehanding the work:

- `execplans`
  - this plan is the governing document and must stay current as implementation
    progresses.
- `rust-router`
  - use it first to route any Rust design, API, dependency, or test-shape
    question to the right deeper skill.
- `rust-errors`
  - use it when mapping Octocrab failures into
    `GitHubError::TokenAcquisitionFailed`
    and `GitHubError::TokenExpired`.
- `rust-types-and-apis`
  - use it when shaping the token value object, helper signatures, and any test
    seam types.
- `en-gb-oxendict-style`
  - use it when editing the design document, user guide, or this ExecPlan so
    documentation stays aligned with repository language rules.

If the implementation grows into orchestration wiring or async lifecycle work,
load additional skills only when the change actually needs them. Do not expand
the skill set speculatively.

## Agent team and ownership

Implementation should use a three-agent team once this draft is approved:

1. Design agent: owns the acquisition seam, expiry-buffer policy, and error
   mapping decisions in `src/github`.
2. Core implementation agent: owns Rust code changes under `src/github/`,
   `Cargo.toml`, and any narrow call-site changes required to keep the code
   compiling.
3. Test and docs agent: owns the new `rstest` submodule, the `rstest-bdd`
   scenarios and helpers, and updates to `docs/podbot-design.md`,
   `docs/users-guide.md`, and `docs/podbot-roadmap.md`.

The design agent remains responsible for integrating discoveries back into this
ExecPlan before each implementation milestone is considered complete.

## Plan of work

### Stage A: resolve the Octocrab expiry gap

Start with a short spike inside `src/github` to choose the concrete acquisition
path. The spike succeeds only when the implementation agent can state, with
code references, how podbot will preserve both the token and its expiry.

Preferred outcome:

1. Add a new token-acquisition helper module, either
   `src/github/installation_token.rs` or another same-feature file under
   `src/github/`.
2. Introduce a podbot-owned value object, for example
   `InstallationAccessToken`, with:
   - the token string,
   - expiry metadata as an RFC 3339 string or parsed time value,
   - redacted `Debug`,
   - `token()` / `into_token()` accessors for Git use, and
   - an `expires_at()` accessor for later refresh scheduling and debugging.
3. Introduce a dedicated async seam, either a new trait or a new helper pair
   analogous to `validate_with_client` and `validate_with_factory`, that allows
   tests to inject a mock installation-token response without constructing a
   live GitHub client.

The preferred production path is to use a typed token response rather than a
bare secret so podbot can preserve `expires_at`. If that proves impossible with
the current public Octocrab surface, stop and record the blocker before moving
to implementation.

### Stage B: implement the public helper and internal policy helpers

Add a new high-level helper with this shape in `src/github/mod.rs`:

```rust
pub async fn installation_token_with_buffer(
    app_id: u64,
    installation_id: u64,
    private_key_path: &Utf8Path,
    buffer: std::time::Duration,
) -> Result<InstallationAccessToken, GitHubError>
```

This helper should:

1. load the private key with `load_private_key`,
2. build an App client with `build_app_client`,
3. request an installation token for `installation_id`,
4. validate that the returned expiry is present and remains outside `buffer`,
5. return the token value object, and
6. map all failures into the existing semantic `GitHubError` variants.

Keep the public helper thin. Extract pure, testable functions for:

- converting `std::time::Duration` into the internal duration type required by
  the chosen client path,
- parsing or validating `expires_at`,
- comparing expiry against `now + buffer`, and
- formatting installation-token acquisition errors.

These helpers are where dependency injection belongs. If a real clock is needed
for the buffer comparison, prefer `mockable::Clock` and enable the `clock`
feature in `Cargo.toml` rather than reaching directly for `Utc::now()` inside
the policy function.

### Stage C: add error classification for installation-token failures

Extend `src/github/classify.rs` so installation-token failures get the same
quality of user guidance that App-authentication failures already receive.

The implementation should distinguish at least these cases:

- 401 or equivalent credential rejection from GitHub,
- 403 permission or installation-scope failures,
- 404 missing installation or App mismatch,
- 5xx or rate-limit failures that imply retry rather than reconfiguration, and
- malformed or missing expiry metadata, which should fail closed before the
  token is returned.

Use `GitHubError::TokenAcquisitionFailed` for GitHub or transport failures. Use
`GitHubError::TokenExpired` when the returned token does not satisfy the
requested buffer.

### Stage D: add focused unit coverage with `rstest`

Create `src/github/installation_token_tests.rs` and register it from
`src/github/mod.rs` under `#[cfg(test)]`.

Unit coverage must include:

1. valid token plus expiry beyond buffer succeeds,
2. expiry inside the buffer returns `TokenExpired`,
3. missing expiry metadata returns a deterministic failure,
4. malformed expiry metadata returns a deterministic failure,
5. installation-token acquisition errors become
   `TokenAcquisitionFailed { message }`,
6. the returned value object exposes the token string but redacts `Debug`, and
7. the high-level helper still propagates private-key and App-client build
   failures unchanged.

Use `rstest` fixtures for repeated setup and parameterize expiry edge cases
rather than duplicating nearly identical tests.

### Stage E: add behavioural coverage with `rstest-bdd`

Add a new feature file: `tests/features/github_installation_token.feature`.

Create a new harness: `tests/bdd_github_installation_token.rs`.

Create a helper directory: `tests/bdd_github_installation_token_helpers/`.

The initial scenario set should cover:

1. `Scenario: Valid credentials produce an installation token`
2. `Scenario: Token expiry inside the buffer is rejected`
3. `Scenario: GitHub rejects installation token acquisition`
4. `Scenario: Missing expiry metadata is rejected`

Follow the established GitHub BDD pattern already used by
`tests/bdd_github_app_client.rs` and
`tests/bdd_github_credential_validation.rs`:

- `ScenarioState` with `Slot<T>`,
- one synchronous `When` step per scenario,
- `StepResult<T> = Result<T, String>`,
- `mockall::mock!` in the helper module rather than relying on the crate-local
  `automock`, and
- assertions against observable outcomes rather than internal fields.

If changes to the feature file do not appear in test runs, use
`cargo clean -p podbot` before rerunning `make test`.

### Stage F: update design and user documentation

Update `docs/podbot-design.md` in the token-management section so it matches
the implemented Step 3.2 contract rather than only showing pseudocode.

Document:

- the new helper name and return value,
- the chosen expiry-buffer policy,
- how expiry metadata is preserved for Step 3.3,
- what counts as `TokenExpired`,
- what remains deferred to the token daemon.

Update `docs/users-guide.md` only for user-visible behaviour:

- required GitHub configuration fields for repository-token acquisition,
- the meaning of installation-token expiry-buffer failures,
- any operator-visible troubleshooting text relevant to token acquisition.

Do not move maintainer-only rationale into the user's guide.

### Stage G: mark the roadmap and run the full gates

Only after the implementation and tests are complete:

1. mark the relevant Step 3.2 task as done in `docs/podbot-roadmap.md`,
2. run the documentation gates,
3. run the required Rust gates, and
4. update `Progress`, `Surprises & Discoveries`, `Decision Log`, and
   `Outcomes & Retrospective`.

Run the gates sequentially, not in parallel:

```bash
set -o pipefail && make fmt 2>&1 | tee /tmp/3-2-1-fmt.log
set -o pipefail && MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint \
  2>&1 | tee /tmp/3-2-1-markdownlint.log
set -o pipefail && make nixie 2>&1 | tee /tmp/3-2-1-nixie.log
set -o pipefail && make check-fmt 2>&1 | tee /tmp/3-2-1-check-fmt.log
set -o pipefail && make lint 2>&1 | tee /tmp/3-2-1-lint.log
set -o pipefail && make test 2>&1 | tee /tmp/3-2-1-test.log
```

If `make test` appears to use stale BDD feature content after editing a
`.feature` file, run:

```bash
cargo clean -p podbot
```

and then rerun the test gate.

## Interfaces and dependencies

The likely Rust changes are:

- `Cargo.toml`
  - add a direct `chrono` dependency only if the chosen path needs duration
    conversion or expiry parsing,
  - enable `mockable`'s `clock` feature if a DI-backed clock is used.
- `src/github/mod.rs`
  - register the new module,
  - expose the public helper and the token value object,
  - add any hidden test seam re-exports required by integration tests.
- `src/github/installation_token.rs`
  - implement the new helper, pure policy functions, and token value object.
- `src/github/classify.rs`
  - add installation-token error mapping.
- `src/github/installation_token_tests.rs`
  - add unit coverage.
- `tests/bdd_github_installation_token.rs`
  - add BDD harness.
- `tests/bdd_github_installation_token_helpers/*`
  - add BDD state, steps, and assertions.
- `tests/features/github_installation_token.feature`
  - add behavioural scenarios.
- `docs/podbot-design.md`, `docs/users-guide.md`, and `docs/podbot-roadmap.md`
  - update the documentation set once the implementation is done.

No other production areas should need changes in this step unless compilation
or documentation coherence requires a narrow follow-up.

## Validation and acceptance

The implementation is complete only when all of the following are true:

1. `installation_token_with_buffer(...)` exists and is documented.
2. The helper returns a token value object that exposes the token string and
   preserves expiry metadata.
3. Tokens whose expiry falls inside the requested buffer fail deterministically.
4. GitHub API or transport failures become semantic installation-token errors.
5. `rstest` unit coverage and `rstest-bdd` scenarios both cover happy and
   unhappy paths.
6. `docs/podbot-design.md` and `docs/users-guide.md` describe the implemented
   behaviour accurately.
7. `docs/podbot-roadmap.md` marks the relevant Step 3.2 task as done.
8. `make check-fmt`, `make lint`, and `make test` all succeed, and the
   documentation gates succeed as well.
