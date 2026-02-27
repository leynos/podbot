# Step 3.1.2: Configure OctocrabBuilder with app\_id and private\_key

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose and big picture

Enable podbot to construct an authenticated Octocrab client from a GitHub App
ID and an RSA private key. After this change, calling
`podbot::github::build_app_client(app_id, private_key)` returns an
`octocrab::Octocrab` instance configured for GitHub App authentication via JWT
signing. This client is the prerequisite for Step 3.2 (installation token
acquisition), where it will be used to call
`octocrab.installation(installation_id).installation_token_with_buffer(...)`.

Observable outcome: running `make test` passes and the following new tests
exist: three unit tests in `src/github/tests.rs` exercise the happy path (valid
App ID and key), a zero App ID edge case, and error message formatting. Two BDD
scenarios in `tests/features/github_app_client.feature` verify end-to-end
behaviour from "given a valid RSA key and an App ID" through "when the App
client is built" to "then the client is ready for use". The function is not yet
wired into the orchestration flow (that is Steps 3.1.3 and 3.1.4).

## Constraints

- Do not modify `src/error.rs`. The `GitHubError::AuthenticationFailed {
  message: String }` variant already exists at lines 149-155 and is sufficient.
- Do not modify `src/config/types.rs`. The `GitHubConfig` struct and its
  validation methods are not consumed by this function — it takes primitives.
- Do not modify `Cargo.toml` dependencies. Both `octocrab = "0.49.5"` and
  `jsonwebtoken = { version = "10.2.0", ... }` are already present.
- Keep all files under 400 lines. Current budgets: `src/github/mod.rs` is 237
  lines (163 remaining), `src/github/tests.rs` is 236 lines (164 remaining).
- Use `?` operator for error propagation; no `.expect()` or `.unwrap()` in
  production code.
- Maintain `missing_docs = "deny"` compliance. All public items require `///`
  rustdoc.
- Use en-GB-oxendict spelling in documentation.
- Use `rstest` for unit tests and `rstest-bdd` v0.5.0 for behavioural tests.
- All existing tests must continue to pass unchanged.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 10 files or more than
  200 net lines, stop and escalate.
- Interface: if a new error variant is required in `src/error.rs`, stop and
  confirm (the existing `AuthenticationFailed` variant should suffice).
- Dependencies: if any new crate dependency is needed, stop and escalate. No
  new dependencies should be required.
- Iterations: if `make lint` or `make test` still fails after three fix passes,
  stop and record blocker evidence.
- Ambiguity: if `Octocrab::builder().app().build()` returns errors that do not
  map cleanly to `GitHubError::AuthenticationFailed`, stop and assess the error
  classification.
- Line budget: if adding the new function and its tests would push either
  `src/github/mod.rs` above 350 lines or `src/github/tests.rs` above 350 lines,
  stop and consider extracting a sub-module before proceeding.

## Risks

- Risk: `Octocrab::builder().app(app_id, key).build()` returns
  `Result<Octocrab, octocrab::Error>`. The `octocrab::Error` enum uses `snafu`
  and is `#[non_exhaustive]`. Converting to `Display` string immediately keeps
  the error variant small. Severity: low. Likelihood: low. Mitigation: convert
  `octocrab::Error` to its `Display` string via `format!("{error}")`, storing
  only the message in `GitHubError::AuthenticationFailed { message }`.

- Risk: `Octocrab::builder().app().build()` may succeed even with invalid
  configuration (e.g., app\_id of 0), making it hard to test failure paths. The
  builder constructs an HTTP client and configures auth state but does not make
  network calls or validate the JWT. Severity: medium. Likelihood: high.
  Mitigation: accept this as a design reality. Unit tests for the happy path
  verify the function returns `Ok(Octocrab)`. The error path is tested by
  observing the error mapping via a direct error construction test. Document
  that credential validation against GitHub happens at token acquisition time
  (Step 3.2), not at client construction time.

- Risk: `octocrab::models::AppId` is a newtype `pub struct AppId(pub u64)` with
  `From<u64>`. Our function must decide whether to accept `u64` or `AppId`.
  Severity: low. Likelihood: certain. Mitigation: accept `u64` in the public
  signature and convert to `AppId` internally via `AppId(app_id)`. This keeps
  the public API free from octocrab coupling.

- Risk: The `build()` method is gated behind
  `#[cfg(feature = "default-client")]`. If octocrab is compiled without this
  feature, `build()` would not be available. Severity: low. Likelihood: very
  low. Mitigation: octocrab's `default-client` feature is enabled by default,
  and `Cargo.toml` lists `octocrab = "0.49.5"` without
  `default-features = false`. No action needed.

## Progress

- [x] (2026-02-27) Draft ExecPlan.
- [x] (2026-02-27) Add `build_app_client` function to `src/github/mod.rs`.
- [x] (2026-02-27) Update module-level doc comment.
- [x] (2026-02-27) Add unit tests to `src/github/tests.rs`.
- [x] (2026-02-27) Run quality gates (`check-fmt`, `lint`, `test`).
- [x] (2026-02-27) Commit core implementation and unit tests.
- [x] (2026-02-27) Create `tests/features/github_app_client.feature`.
- [x] (2026-02-27) Create BDD harness `tests/bdd_github_app_client.rs`.
- [x] (2026-02-27) Create `tests/bdd_github_app_client_helpers/` (state, steps,
  assertions, mod).
- [x] (2026-02-27) Run quality gates.
- [x] (2026-02-27) Commit BDD tests.
- [x] (2026-02-27) Update `docs/podbot-design.md` with App client construction
  contract.
- [x] (2026-02-27) Update `docs/podbot-roadmap.md` to mark task complete.
- [x] (2026-02-27) Run documentation gates (`markdownlint`, `fmt`, `nixie`).
- [x] (2026-02-27) Commit documentation updates.
- [x] (2026-02-27) Finalise outcomes and retrospective.

## Surprises and discoveries

- Observation: `Octocrab::builder().build()` requires a Tokio runtime context
  at call time, even though the method is synchronous. This is because Tower's
  `Buffer` service (used internally by Octocrab's retry/rate-limiting layer)
  spawns a background task via `tokio::spawn` during construction. Evidence:
  unit tests panicked with "there is no reactor running, must be called from
  the context of a Tokio 1.x runtime" at
  `tower-0.5.3/src/buffer/service.rs:57:9`. The fix is to create a
  `tokio::runtime::Runtime` and call `rt.enter()` before building. Impact: both
  unit tests and BDD When steps needed explicit runtime setup. This pattern
  already existed in the codebase at
  `tests/bdd_credential_injection_helpers/steps.rs:115`.

- Observation: `Slot<T>::get()` returns `Option<T>` (cloned), not
  `Option<&T>`. Calling `.copied()` on `Option<u64>` is a type error. Evidence:
  clippy reported `no method named 'copied' found for enum Option<u64>`.
  Impact: removed the `.copied()` call in BDD steps; passed `&key_path` to
  `load_private_key` since `get()` returns an owned `Utf8PathBuf`.

- Observation: the `#[given]` macro in rstest-bdd generates wrapper code that
  returns `StepResult`, triggering `clippy::unnecessary_wraps` when the inner
  function body cannot fail. The established codebase pattern uses
  `#[expect(clippy::unnecessary_wraps)]` after the step attribute. Evidence:
  clippy reported `this function's return value is unnecessary` for the
  `set_app_id` step function. Impact: added the `#[expect]` attribute with the
  standard reason string.

## Decision log

- Decision: accept `u64` in the public signature rather than
  `octocrab::models::AppId`. Rationale: keeps the public API decoupled from
  octocrab's type system. The `github` module is internal but other internal
  code calling it should not need to import octocrab types. The conversion
  `AppId(app_id)` is trivial because the inner field is `pub`. Date/Author:
  2026-02-27 / DevBoxer.

- Decision: do not introduce a trait abstraction for `Octocrab` at this stage.
  Rationale: the `build()` method is synchronous and local. There are no
  network calls to mock. A trait (e.g., `GitHubClient`) would be needed in Step
  3.2 when async token acquisition begins. Introducing it now would be
  premature abstraction. Date/Author: 2026-02-27 / DevBoxer.

- Decision: create a separate BDD feature file rather than extending
  `github_private_key.feature`. Rationale: the concerns are distinct. Private
  key loading validates PEM file handling. App client construction validates
  Octocrab builder configuration. Separate feature files follow the project's
  "group by feature" principle. Date/Author: 2026-02-27 / DevBoxer.

- Decision: do not attempt to test the builder error path with a real
  `octocrab::Error`. Rationale: `Octocrab::builder().app().build()` is
  difficult to make fail in a test environment. The builder failure paths
  involve TLS connector initialisation (`with_native_roots()`), which depends
  on the operating system certificate store. Artificially breaking the
  certificate store in tests would be fragile and non-portable. Instead, we
  test the error format directly and rely on the type system to guarantee that
  `map_err` produces the correct variant. Date/Author: 2026-02-27 / DevBoxer.

## Outcomes and retrospective

All deliverables achieved. The `build_app_client` function is implemented,
tested (3 unit tests, 2 BDD scenarios), and documented. All quality gates pass:
`make check-fmt`, `make lint`, `make test`, `make markdownlint`, `make nixie`.

The main surprise was that `Octocrab::builder().build()` requires a Tokio
runtime context due to Tower's `Buffer` service spawning a background task.
This was not documented in Octocrab's API documentation but was discovered
through test failures. The fix (creating an explicit `Runtime` and calling
`rt.enter()`) was straightforward and consistent with existing patterns in the
codebase.

The `Slot::get()` API returning `Option<T>` (cloned) rather than `Option<&T>`
caused a minor type error in BDD steps. This is worth recording for future BDD
test authors.

Lessons for future steps: Step 3.2 (installation token acquisition) will need
async test support and likely a trait abstraction for `Octocrab` to enable
mocking of network calls. The Tokio runtime requirement discovered here will
carry forward naturally since Step 3.2 is inherently async.

## Context and orientation

Podbot is a Rust application (edition 2024, minimum supported Rust version
1.88) that creates secure containers for AI coding agents. The project is
structured as a dual-delivery library and CLI binary. The codebase uses Clippy
pedantic with strict lint settings: `expect_used`, `unwrap_used`,
`indexing_slicing`, `print_stdout`, and `print_stderr` are all denied.

Key files for this task:

`src/github/mod.rs` (237 lines) is the GitHub module. It currently exposes one
public function,
`load_private_key(key_path: &Utf8Path) -> Result<EncodingKey, GitHubError>`,
plus five private helpers for PEM validation. The module begins with a `//!`
doc comment (lines 1-9), has `use` imports (lines 11-18), constant definitions
(lines 20-36), and function implementations (lines 38-234). It ends with
`#[cfg(test)] mod tests;` on lines 236-237. The new `build_app_client` function
will be added here.

`src/github/tests.rs` (236 lines) contains unit tests for the GitHub module. It
uses `rstest` fixtures (`valid_rsa_pem`, `ec_pem`, `ed25519_pem`,
`temp_key_dir`) and 14 test functions covering `load_private_key` happy and
unhappy paths. New unit tests for `build_app_client` will be added here. The
file uses `use super::*;` to access private module members.

`src/error.rs` (466 lines) defines
`GitHubError::AuthenticationFailed { message: String }` at lines 149-155. This
variant exists but is currently unused. This plan will make it the error type
for builder failures. The file will not be modified.

`src/config/types.rs` (241 lines) defines `GitHubConfig` with fields
`app_id: Option<u64>`, `installation_id: Option<u64>`,
`private_key_path: Option<Utf8PathBuf>`. The `app_id` value from this struct
will eventually be passed to `build_app_client`, but this plan does not wire
them together. This file will not be modified.

`tests/features/github_private_key.feature` (64 lines) is the existing BDD
feature file for private key loading. A new feature file will be created for
App client building.

`tests/bdd_github_private_key.rs` (89 lines) and
`tests/bdd_github_private_key_helpers/` (4 files, 242 lines total) are the
existing BDD harness and helpers for private key scenarios. The new BDD tests
for App client building will follow this exact structural pattern.

Octocrab's `OctocrabBuilder::app()` method (at `lib.rs:644` in octocrab
v0.49.5) accepts `(AppId, jsonwebtoken::EncodingKey)` where `AppId` is
`octocrab::models::AppId`, a newtype `pub struct AppId(pub u64)` implementing
`From<u64>`, `Clone`, `Copy`, `Debug`, `Display`, `Eq`, `PartialEq`,
`Serialize`, and `Deserialize`. The `build()` method (at `lib.rs:729`) returns
`Result<Octocrab, octocrab::Error>` and is gated behind
`#[cfg(feature = "default-client")]` (enabled by default). Critically,
`build()` is synchronous and does not make network calls; it constructs an HTTP
client with TLS connectors and configures the auth state.

## Agent team and ownership

Implementation uses a single integrator agent that:

- owns all new files (BDD harness, helpers, feature file);
- modifies `src/github/mod.rs` and `src/github/tests.rs` for the core
  function and unit tests;
- updates documentation files (`podbot-design.md`, `podbot-roadmap.md`);
- runs quality gates and commits each logical slice.

## Plan of work

### Stage A: Core implementation in `src/github/mod.rs`

Add two imports after line 18 (`use crate::error::GitHubError;`):

```rust
use octocrab::models::AppId;
use octocrab::Octocrab;
```

Add a public function after `load_private_key` (after line 64, before the
private function `load_private_key_from_dir` at line 70). The function
constructs an `OctocrabBuilder`, sets App authentication, and builds the client:

```rust
/// Build an authenticated Octocrab client for GitHub App operations.
///
/// Configures `OctocrabBuilder` with the given App ID and RSA private
/// key, producing a client ready for JWT signing and installation token
/// acquisition.
///
/// The client is constructed synchronously and does not make network
/// calls. Credential validation against GitHub occurs later, when the
/// client is used to acquire an installation token (Step 3.2).
///
/// # Errors
///
/// Returns [`GitHubError::AuthenticationFailed`] if the Octocrab
/// builder fails to construct the HTTP client (for example, due to TLS
/// initialisation failure).
pub fn build_app_client(
    app_id: u64,
    private_key: EncodingKey,
) -> Result<Octocrab, GitHubError> {
    Octocrab::builder()
        .app(AppId(app_id), private_key)
        .build()
        .map_err(|error| GitHubError::AuthenticationFailed {
            message: format!("failed to build GitHub App client: {error}"),
        })
}
```

Update the module-level `//!` doc comment (line 3) to mention the new function.
Change:

```plaintext
//! This module handles loading GitHub App credentials for JWT signing.
```

to:

```plaintext
//! This module handles loading GitHub App credentials for JWT signing
//! and constructing an authenticated Octocrab client for App operations.
```

This adds approximately 25 lines to `src/github/mod.rs` (237 + 25 = ~262 lines,
well within the 400-line limit).

Validation: `make check-fmt && make lint` pass.

### Stage B: Unit tests in `src/github/tests.rs`

Add three new unit tests after the existing tests (after line 236). The tests
use the existing `valid_rsa_pem` and `temp_key_dir` fixtures.

Test 1 (`build_app_client_with_valid_key_succeeds`): load the test RSA key via
`load_private_key_from_dir`, then call `build_app_client(12345, key)` and
assert `is_ok()`.

Test 2 (`build_app_client_with_zero_app_id_succeeds`): call
`build_app_client(0, key)` and verify it returns `Ok`. This documents the known
behaviour that the builder does not validate `app_id` — validation against
GitHub happens at token acquisition time.

Test 3 (`authentication_failed_error_includes_context`): construct a
`GitHubError::AuthenticationFailed` error directly and verify its `Display`
output includes the expected format strings. This confirms the error message
format without needing to provoke a real builder failure.

Estimated addition: approximately 45 lines. `tests.rs` would go from 236 to
~281 lines, well within the 400-line limit.

Validation: `make test` passes with all new tests green.

### Stage C: BDD tests

Create `tests/features/github_app_client.feature` with two scenarios covering
the happy path and the zero App ID edge case.

Create `tests/bdd_github_app_client.rs` as the BDD harness, following the exact
pattern of `tests/bdd_github_private_key.rs`: import the helper module,
re-export the state type and fixture, and use the `#[scenario]` macro to bind
each scenario.

Create the helper directory `tests/bdd_github_app_client_helpers/` with four
files:

`mod.rs` re-exports the state type and fixture function from `state.rs`.

`state.rs` defines `GitHubAppClientState` with
`#[derive(Default, ScenarioState)]` and `Slot`-based fields:
`temp_dir: Slot<Arc<TempDir>>`, `key_path: Slot<Utf8PathBuf>`,
`app_id: Slot<u64>`, and `outcome: Slot<ClientBuildOutcome>`. It also defines
the `ClientBuildOutcome` enum (with `Success` and `Failed { message }`
variants), the `StepResult<T>` type alias, and the `github_app_client_state`
fixture function.

`steps.rs` defines Given and When step functions. The Given steps set up the
RSA key file (reusing the `open_temp_dir` helper pattern from
`bdd_github_private_key_helpers/steps.rs`) and set the App ID. The When step
calls `podbot::github::load_private_key` followed by
`podbot::github::build_app_client`, storing the outcome in state. All step
function parameters use the fixture name `github_app_client_state` exactly (not
`state`).

`assertions.rs` defines Then step functions that match on `ClientBuildOutcome`
using the `StepResult` pattern (no `expect` or `unwrap`).

Important BDD patterns enforced:

- `StepResult<T> = Result<T, String>` with `ok_or_else` / `map_err` instead of
  `expect` or `unwrap` (clippy `expect_used` is denied).
- Parameter names match fixture names exactly (rstest-bdd matches by name).
- Feature file text uses unquoted values for `{param}` captures.
- Feature files are read at compile time; `cargo clean -p podbot` is needed
  after modifying them.

Validation: `make test` passes with BDD scenarios green.

### Stage D: Documentation updates

Update `docs/podbot-design.md` by adding an "App client construction contract"
paragraph after the existing "Private key loading contract" section (after line
253). The paragraph describes:

- The function signature and what it accepts.
- That construction is synchronous and does not make network calls.
- That credential validation occurs at token acquisition time.
- That builder failures are mapped to `GitHubError::AuthenticationFailed`.

Update `docs/podbot-roadmap.md` at line 150 to mark the task as done: change
`- [ ] Configure OctocrabBuilder with app_id and private_key.` to
`- [x] Configure OctocrabBuilder with app_id and private_key.`.

`docs/users-guide.md` does not need changes because `build_app_client` is an
internal library API, not a user-facing feature.

Validation: `make markdownlint`, `make fmt`, `make nixie` all pass.

### Stage E: Quality gates and commits

Commit in three logical slices following the established pattern from Step
3.1.1:

1. Core implementation plus unit tests (Stages A and B).
2. BDD tests (Stage C).
3. Documentation plus roadmap (Stage D).

Each commit gated with:

```bash
set -o pipefail; make check-fmt 2>&1 | tee /tmp/check-fmt-3-1-2.out
set -o pipefail; make lint 2>&1 | tee /tmp/lint-3-1-2.out
set -o pipefail; make test 2>&1 | tee /tmp/test-3-1-2.out
```

Documentation commit additionally gated with:

```bash
set -o pipefail; MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint \
  2>&1 | tee /tmp/markdownlint-3-1-2.out
set -o pipefail; make fmt 2>&1 | tee /tmp/fmt-3-1-2.out
set -o pipefail; make nixie 2>&1 | tee /tmp/nixie-3-1-2.out
```

## Interfaces and dependencies

No new dependencies. The function uses:

- `octocrab::Octocrab` (the client type, already a dependency).
- `octocrab::models::AppId` (newtype for `u64`, already a transitive type).
- `jsonwebtoken::EncodingKey` (already a direct dependency).
- `crate::error::GitHubError::AuthenticationFailed` (already defined, currently
  unused).

New public interface added to `podbot::github`:

```rust
pub fn build_app_client(
    app_id: u64,
    private_key: jsonwebtoken::EncodingKey,
) -> Result<octocrab::Octocrab, crate::error::GitHubError>
```

## Validation and acceptance

Running `make test` passes and the following new tests exist:

Unit tests in `src/github/tests.rs` (3 new cases):

- `tests::build_app_client_with_valid_key_succeeds`
- `tests::build_app_client_with_zero_app_id_succeeds`
- `tests::authentication_failed_error_includes_context`

BDD scenarios in `tests/bdd_github_app_client.rs` (2 scenarios):

- "Valid credentials produce an App client"
- "Zero App ID is accepted by the builder"

Quality criteria:

- `make check-fmt` passes.
- `make lint` passes (clippy with warnings denied, including pedantic,
  `expect_used`, `unwrap_used`, `result_large_err`, `missing_const_for_fn`,
  `must_use_candidate`).
- `make test` passes (all existing plus new tests).
- `make markdownlint` passes.
- `make nixie` passes.
- Roadmap task "Configure OctocrabBuilder with app\_id and private\_key" is
  marked `[x]`.

## Idempotence and recovery

All stages are re-runnable. `cargo check` and `make test` are idempotent. No
destructive operations are involved. If a stage fails, the working tree can be
reset to the previous commit and the stage re-attempted.

## Clippy lint considerations

The following clippy lints may trigger and require attention:

`missing_const_for_fn`: the `build_app_client` function body calls
`Octocrab::builder()` which is not `const`, so this lint should not fire.

`must_use_candidate`: the function returns `Result`, which already has
`#[must_use]` in the standard library. This should not fire on functions
returning `Result`.

`needless_pass_by_value`: `EncodingKey` does not implement `Copy` and the
builder's `.app()` method takes it by value (consuming it). Passing by value is
correct. The `u64` for `app_id` is `Copy` and trivially passed by value.

`result_large_err`: `GitHubError::AuthenticationFailed { message: String }`
stores only a `String` (24 bytes on 64-bit). The `GitHubError` enum's largest
variant is `PrivateKeyLoadFailed` which has `PathBuf` (24 bytes) plus `String`
(24 bytes) equals 48 bytes plus discriminant. This should be under the clippy
threshold (default 128 bytes).
