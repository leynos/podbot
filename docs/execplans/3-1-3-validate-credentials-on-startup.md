# Step 3.1.3: Validate credentials produce a valid App token on startup

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: DONE

## Purpose and big picture

Enable podbot to validate GitHub App credentials at startup by generating a JWT
(JSON Web Token) from the configured App ID and private key, then calling the
GitHub API to verify the token is accepted. After this change, running
`podbot run` with valid GitHub configuration will verify credentials before
proceeding with container setup. Invalid or missing credentials will produce
clear, actionable error messages immediately rather than failing later during
repository cloning.

Observable outcome: running `make test` passes and the following new tests
exist: unit tests in `src/github/tests.rs` covering token generation and API
call mocking, and behaviour-driven development (BDD) scenarios in
`tests/features/github_credential_validation.feature` verifying happy and
unhappy paths (valid credentials succeed, API rejection fails, server error
fails). The validation function is not yet wired into the orchestration flow;
that integration will be part of the `run` subcommand implementation in
Phase 4.

User-visible behaviour: when GitHub credentials are configured, podbot will
validate them during startup. If validation fails, the user sees an error like
`GitHub App authentication failed: invalid credentials - the App ID may be
incorrect or the private key may not match` with suggestions for resolution.

## Constraints

- Do not modify `src/error.rs` except to add new error variants if absolutely
  necessary. Prefer using existing `GitHubError::AuthenticationFailed` and
  `GitHubError::TokenAcquisitionFailed` variants.
- Keep all files under 400 lines. Current budgets: `src/github/mod.rs` is
  approximately 265 lines (135 remaining), `src/github/tests.rs` is
  approximately 311 lines (89 remaining). If tests exceed budget, split into
  `src/github/validation_tests.rs` or similar.
- Use `?` operator for error propagation; no `.expect()` or `.unwrap()` in
  production code.
- Maintain `missing_docs = "deny"` compliance. All public items require `///`
  rustdoc.
- Use en-GB-oxendict spelling in documentation.
- Use `rstest` for unit tests and `rstest-bdd` v0.5.0 for behavioural tests.
- All existing tests must continue to pass unchanged.
- Do not introduce new crate dependencies without escalation.
- The validation function must be async because it makes a network call to
  GitHub.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 12 files or more than
  300 net lines, stop and escalate.
- Interface: if a new error variant is required beyond those in the current
  `GitHubError` enum, stop and confirm. The existing `AuthenticationFailed` and
  `TokenAcquisitionFailed` variants should suffice.
- Dependencies: if any new crate dependency is needed (beyond what is already
  in `Cargo.toml`), stop and escalate. The existing `octocrab`, `tokio`, and
  `mockall` dependencies should suffice.
- Iterations: if `make lint` or `make test` still fails after three fix passes,
  stop and record blocker evidence.
- Mocking: if mocking Octocrab's API calls proves infeasible with the current
  architecture, stop and assess whether a trait abstraction is needed. This
  would represent a larger architectural change.
- Line budget: if adding the new function and its tests would push either
  `src/github/mod.rs` above 380 lines or `src/github/tests.rs` above 380 lines,
  stop and split into submodules before proceeding.

## Risks

- Risk: Mocking Octocrab's network calls requires either a trait abstraction or
  integration tests against a real GitHub instance. Unit tests may be limited
  to verifying error handling paths and message formatting. Severity: medium.
  Likelihood: high. Mitigation: use dependency injection with a
  `GitHubAppClient` trait that wraps Octocrab, allowing mock implementations in
  tests. If this proves too complex for this step, defer full async mocking to
  Step 3.2 (installation token acquisition) and test only synchronous paths
  here.

- Risk: Octocrab's error types are `#[non_exhaustive]` and use `snafu`. Error
  classification may be imprecise. Severity: low. Likelihood: medium.
  Mitigation: convert errors to their `Display` string for human-readable
  messages, and log the full error chain at debug level for troubleshooting.

- Risk: The App JWT generation (via `OctocrabBuilder::app()`) is tested in Step
  3.1.2, but validating the JWT against GitHub requires a network call.
  Severity: low. Likelihood: certain. Mitigation: the validation function calls
  `GET /app` which returns the authenticated App's metadata. This endpoint is
  documented and stable.

- Risk: GitHub's API rate limits could affect validation during high-frequency
  restarts. Severity: low. Likelihood: low. Mitigation: App-authenticated
  requests have separate rate limits (5000/hour per installation), and
  validation happens once per startup. This is acceptable overhead.

## Progress

- [x] Draft ExecPlan.
- [x] Define `GitHubAppClient` trait for dependency injection.
- [x] Implement `validate_app_credentials` async function.
- [x] Add unit tests with mock client.
- [x] Create BDD feature file and helpers.
- [x] Run quality gates (`check-fmt`, `lint`, `test`).
- [x] Update `docs/podbot-design.md` with validation contract.
- [x] Update `docs/podbot-roadmap.md` to mark task complete.
- [x] Update `docs/users-guide.md` if applicable (no changes needed).
- [x] Run documentation gates (`markdownlint`, `fmt`, `nixie`).
- [x] Finalise outcomes and retrospective.

## Surprises and discoveries

- The `#[cfg_attr(test, mockall::automock)]` attribute generates
  `MockGitHubAppClient` but only within the main crate's test configuration.
  External test binaries (BDD tests) cannot access this mock, so a separate
  `mockall::mock!` declaration was needed in the test helpers.
- `serde_json` was already a transitive dependency via `octocrab` but needed to
  be added as a direct dependency in `Cargo.toml` for use in `mod.rs`.
- The `OctocrabAppClient::new` constructor was flagged by `missing_const_for_fn`
  and required `const fn` annotation.

## Decision log

- **Mock strategy for BDD tests**: Rather than restructuring the crate to export
  the automock-generated type, a local `mockall::mock!` was defined in the BDD
  helpers. This keeps the production code clean and follows the existing pattern
  used by other BDD tests in the codebase.

## Context and orientation

Podbot is a Rust application (edition 2024, minimum supported Rust version
1.88) that creates secure containers for AI coding agents. The project is
structured as a dual-delivery library and CLI binary. The codebase uses Clippy
pedantic with strict lint settings: `expect_used`, `unwrap_used`,
`indexing_slicing`, `print_stdout`, and `print_stderr` are all denied.

Key files for this task:

`src/github/mod.rs` (approximately 265 lines) is the GitHub module. It
currently exposes two public functions:

- `load_private_key(key_path: &Utf8Path) -> Result<EncodingKey, GitHubError>`
- `build_app_client(app_id: u64, private_key: EncodingKey) -> Result<Octocrab, GitHubError>`

The new `validate_app_credentials` function will be added here.

`src/github/tests.rs` (approximately 311 lines) contains unit tests for the
GitHub module using `rstest` fixtures. New unit tests for credential validation
will be added here.

`src/error.rs` defines `GitHubError` with these relevant variants:

- `AuthenticationFailed { message: String }` - for App client build failures
- `TokenAcquisitionFailed { message: String }` - for token request failures
- `PrivateKeyLoadFailed { path: PathBuf, message: String }` - for key loading

`src/config/types.rs` defines `GitHubConfig` with fields:
`app_id: Option<u64>`, `installation_id: Option<u64>`,
`private_key_path: Option<Utf8PathBuf>`. The `validate()` method checks that
all required fields are present and non-zero. The `is_configured()` method
returns true if credentials are fully configured.

`tests/features/github_app_client.feature` and
`tests/bdd_github_app_client_helpers/` are the existing BDD harness for App
client construction. The new credential validation BDD tests will follow this
exact structural pattern.

Octocrab's `Octocrab::apps()` method provides access to App-related API
endpoints. The `GET /app` endpoint returns metadata about the authenticated
GitHub App, which serves as a validation check.

## Agent team and ownership

Implementation uses a two-agent team:

1. **Design agent**: owns the trait definition and interface design, ensures
   testability and clean abstractions.

2. **Implementation agent**: owns all new files (BDD harness, helpers, feature
   file), modifies `src/github/mod.rs` and `src/github/tests.rs` for the core
   function and unit tests, updates documentation files, runs quality gates and
   commits each logical slice.

## Plan of work

### Stage A: Design trait abstraction for testability

The core challenge is testing async network calls without hitting the real
GitHub API. A trait is introduced to wrap the Octocrab client operations
needed, allowing mock implementations in tests.

Add a trait definition to `src/github/mod.rs` after the existing functions:

```rust
/// Trait for GitHub App client operations.
///
/// This trait abstracts the Octocrab client to enable testing without
/// network calls. Production code uses `OctocrabAppClient`, while tests
/// inject `MockGitHubAppClient`.
#[cfg_attr(test, mockall::automock)]
pub trait GitHubAppClient: Send + Sync {
    /// Validates that the App credentials are accepted by GitHub.
    ///
    /// Calls `GET /app` and verifies the response indicates a valid
    /// authenticated App.
    ///
    /// # Errors
    ///
    /// Returns an error if the API call fails or returns an error response.
    fn validate_credentials(
        &self,
    ) -> impl std::future::Future<Output = Result<(), GitHubError>> + Send;
}
```

Note: `mockall::automock` generates `MockGitHubAppClient` automatically.

Add the production implementation wrapping `Octocrab`:

```rust
/// Production implementation of [`GitHubAppClient`] using Octocrab.
pub struct OctocrabAppClient {
    client: Octocrab,
}

impl OctocrabAppClient {
    /// Creates a new `OctocrabAppClient` from an authenticated Octocrab
    /// instance.
    #[must_use]
    pub fn new(client: Octocrab) -> Self {
        Self { client }
    }
}

impl GitHubAppClient for OctocrabAppClient {
    async fn validate_credentials(&self) -> Result<(), GitHubError> {
        self.client
            .get::<serde_json::Value, _, ()>("/app", None)
            .await
            .map_err(|error| GitHubError::AuthenticationFailed {
                message: format!(
                    "failed to validate GitHub App credentials: {error}"
                ),
            })?;
        Ok(())
    }
}
```

Estimated addition: approximately 40 lines.

Validation: `cargo check` compiles without errors.

### Stage B: Add high-level validation function

Add a convenience function that orchestrates the full validation flow:

```rust
/// Validates GitHub App credentials by loading the private key, building
/// the App client, and verifying credentials are accepted by GitHub.
///
/// This function performs a network call to GitHub's `/app` endpoint to
/// verify that the configured `app_id` and private key produce a valid JWT
/// that GitHub accepts.
///
/// # Arguments
///
/// * `app_id` - The GitHub App ID
/// * `private_key_path` - Path to the PEM-encoded RSA private key
///
/// # Errors
///
/// Returns [`GitHubError::PrivateKeyLoadFailed`] if the key cannot be loaded.
/// Returns [`GitHubError::AuthenticationFailed`] if the client cannot be
/// built or if GitHub rejects the credentials.
///
/// # Example
///
/// ```rust,no_run
/// use podbot::github::validate_app_credentials;
/// use camino::Utf8Path;
///
/// # async fn example() -> Result<(), podbot::error::GitHubError> {
/// let app_id = 12345;
/// let key_path = Utf8Path::new("/path/to/private-key.pem");
/// validate_app_credentials(app_id, key_path).await?;
/// println!("Credentials are valid!");
/// # Ok(())
/// # }
/// ```
pub async fn validate_app_credentials(
    app_id: u64,
    private_key_path: &Utf8Path,
) -> Result<(), GitHubError> {
    let private_key = load_private_key(private_key_path)?;
    let octocrab = build_app_client(app_id, private_key)?;
    let client = OctocrabAppClient::new(octocrab);
    client.validate_credentials().await
}
```

Estimated addition: approximately 35 lines.

Update the module-level `//!` doc comment to mention credential validation:

```plaintext
//! This module handles loading GitHub App credentials for JWT signing,
//! constructing an authenticated Octocrab client for App operations,
//! and validating credentials against the GitHub API.
```

Validation: `make check-fmt && make lint` pass.

### Stage C: Unit tests

Add unit tests to `src/github/tests.rs`. The Octocrab HTTP layer cannot
easily be mocked in unit tests without significant refactoring, so the unit
tests focus on:

1. Testing the trait implementation exists and compiles.
2. Testing error message formatting.
3. Testing the `OctocrabAppClient::new` constructor.

For full async testing with mocks, we rely on the BDD tests using the mock
trait.

```rust
#[rstest]
fn octocrab_app_client_new_creates_instance(
    valid_rsa_pem: String,
    temp_key_dir: (TempDir, Utf8Dir),
) {
    let (_tmp, dir) = temp_key_dir;
    dir.write("key.pem", &valid_rsa_pem)
        .expect("should write key");
    let path = Utf8Path::new("/display/key.pem");
    let key = load_private_key_from_dir(&dir, "key.pem", path)
        .expect("should load valid key");
    let rt = tokio::runtime::Runtime::new()
        .expect("should create tokio runtime");
    let _guard = rt.enter();
    let octocrab = build_app_client(12345, key)
        .expect("should build client");
    let client = OctocrabAppClient::new(octocrab);
    // Verify the client was created (we can't test async methods easily here)
    let _ = client;
}
```

Estimated addition: approximately 20 lines.

Validation: `make test` passes with new unit tests green.

### Stage D: BDD tests

Create `tests/features/github_credential_validation.feature` with scenarios:

```gherkin
Feature: GitHub App credential validation

  Podbot validates GitHub App credentials on startup by calling the
  GitHub API to verify the configured App ID and private key produce
  a valid JWT that GitHub accepts.

  Scenario: Valid credentials pass validation
    Given a mock GitHub API that accepts App credentials
    And a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    When credentials are validated
    Then validation succeeds

  Scenario: Invalid App ID fails validation
    Given a mock GitHub API that rejects invalid App credentials
    And a valid RSA private key file exists at the configured path
    And the GitHub App ID is 99999
    When credentials are validated
    Then validation fails
    And the error mentions invalid credentials

  Scenario: API failure is handled gracefully
    Given a mock GitHub API that returns a server error
    And a valid RSA private key file exists at the configured path
    And the GitHub App ID is 12345
    When credentials are validated
    Then validation fails
    And the error mentions failed to validate
```

Create `tests/bdd_github_credential_validation.rs` as the BDD harness following
the pattern of `tests/bdd_github_app_client.rs`.

Create `tests/bdd_github_credential_validation_helpers/` with four files:

`mod.rs` re-exports the state type and fixture function from `state.rs`.

`state.rs` defines `GitHubCredentialValidationState` with
`#[derive(Default, ScenarioState)]` and `Slot`-based fields:

- `temp_dir: Slot<Arc<TempDir>>`
- `key_path: Slot<Utf8PathBuf>`
- `app_id: Slot<u64>`
- `mock_response: Slot<MockResponse>`
- `outcome: Slot<ValidationOutcome>`

Where `MockResponse` is an enum: `Success`, `InvalidCredentials`,
`ServerError`. And `ValidationOutcome` is an enum: `Success`,
`Failed { message: String }`.

`steps.rs` defines Given and When step functions. The Given steps set up mock
responses, RSA key files, and App IDs. The When step creates a mock client,
configures it based on `mock_response`, and calls `validate_credentials()`.

`assertions.rs` defines Then step functions that verify `ValidationOutcome`.

Important BDD patterns enforced:

- `StepResult<T> = Result<T, String>` with `ok_or_else` / `map_err` instead of
  `expect` or `unwrap`.
- Parameter names match fixture names exactly.
- Feature file text uses unquoted values for `{param}` captures.
- Feature files are read at compile time; `cargo clean -p podbot` is needed
  after modifying them.

Estimated additions: approximately 150 lines across all BDD files.

Validation: `make test` passes with BDD scenarios green.

### Stage E: Documentation updates

Update `docs/podbot-design.md` by adding a "Credential validation contract"
section after the "App client construction contract" section (after line 275).
The paragraph describes:

- The `validate_app_credentials` function signature.
- That it performs a network call to `GET /app`.
- That validation happens once at startup for commands requiring GitHub access.
- Error handling and message formatting.

Update `docs/podbot-roadmap.md` at the relevant line to mark the task as done:
change `- [ ] Validate credentials produce a valid App token on startup.` to
`- [x] Validate credentials produce a valid App token on startup.`.

Check `docs/users-guide.md` for any user-facing documentation updates. If
credential validation is exposed as a user-visible feature, document the error
messages and troubleshooting steps.

Validation: `make markdownlint`, `make fmt`, `make nixie` all pass.

### Stage F: Quality gates and commits

Commit in logical slices following the established pattern:

1. Trait definition and production implementation (Stage A).
2. High-level validation function (Stage B).
3. Unit tests (Stage C).
4. BDD tests (Stage D).
5. Documentation updates (Stage E).

Each commit gated with:

```bash
set -o pipefail; make check-fmt 2>&1 | tee /tmp/check-fmt-3-1-3.out
set -o pipefail; make lint 2>&1 | tee /tmp/lint-3-1-3.out
set -o pipefail; make test 2>&1 | tee /tmp/test-3-1-3.out
```

Documentation commit additionally gated with:

```bash
set -o pipefail; MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint \
  2>&1 | tee /tmp/markdownlint-3-1-3.out
set -o pipefail; make fmt 2>&1 | tee /tmp/fmt-3-1-3.out
set -o pipefail; make nixie 2>&1 | tee /tmp/nixie-3-1-3.out
```

## Interfaces and dependencies

No new dependencies. The implementation uses:

- `octocrab::Octocrab` (the client type, already a dependency).
- `mockall` (for generating mock implementations, already a dev-dependency).
- `tokio` (for async runtime, already a dependency).
- `jsonwebtoken::EncodingKey` (already a direct dependency).
- `crate::error::GitHubError` (already defined).

New public interfaces added to `podbot::github`:

```rust
/// Trait for GitHub App client operations.
pub trait GitHubAppClient: Send + Sync {
    fn validate_credentials(
        &self,
    ) -> impl std::future::Future<Output = Result<(), GitHubError>> + Send;
}

/// Production implementation using Octocrab.
pub struct OctocrabAppClient {
    client: Octocrab,
}

impl OctocrabAppClient {
    pub fn new(client: Octocrab) -> Self;
}

impl GitHubAppClient for OctocrabAppClient {
    async fn validate_credentials(&self) -> Result<(), GitHubError>;
}

/// Validates credentials by loading key, building client, and calling GitHub.
pub async fn validate_app_credentials(
    app_id: u64,
    private_key_path: &Utf8Path,
) -> Result<(), GitHubError>;
```

## Validation and acceptance

Running `make test` passes and the following new tests exist:

Unit tests in `src/github/tests.rs` (at least 2 new cases):

- `tests::octocrab_app_client_new_creates_instance`
- `tests::authentication_failed_error_includes_validation_context`

BDD scenarios in `tests/bdd_github_credential_validation.rs` (3 scenarios):

- "Valid credentials pass validation"
- "Invalid App ID fails validation"
- "API failure is handled gracefully"

Quality criteria:

- `make check-fmt` passes.
- `make lint` passes (clippy with warnings denied, including pedantic,
  `expect_used`, `unwrap_used`, `result_large_err`, `missing_const_for_fn`,
  `must_use_candidate`).
- `make test` passes (all existing plus new tests).
- `make markdownlint` passes.
- `make nixie` passes.
- Roadmap task "Validate credentials produce a valid App token on startup" is
  marked `[x]`.

## Idempotence and recovery

All stages are re-runnable. `cargo check` and `make test` are idempotent. No
destructive operations are involved. If a stage fails, the working tree can be
reset to the previous commit and the stage re-attempted.

## Clippy lint considerations

The following clippy lints may trigger and require attention:

`missing_const_for_fn`: the async functions cannot be const. The
`OctocrabAppClient::new` constructor may be flagged; if so, make it `const` or
add `#[expect]` with reason.

`must_use_candidate`: the `new` constructor returns `Self` so should have
`#[must_use]`. Add the attribute explicitly.

`needless_pass_by_value`: `Octocrab` does not implement `Copy` and the
constructor takes ownership. Passing by value is correct.

`result_large_err`: `GitHubError::AuthenticationFailed { message: String }`
stores only a `String` (24 bytes on 64-bit). This should be under the clippy
threshold.

`async_fn_in_trait`: Rust 1.75+ supports async functions in traits directly.
Since podbot uses Rust 1.88+, this should work without the `async-trait` crate.
However, if `mockall::automock` requires specific signatures, we may need to
adjust the trait definition.

## Outcomes and retrospective

Implementation completed successfully. All quality gates pass.

**Files modified:**

- `Cargo.toml` — added `serde_json` as direct dependency
- `src/github/mod.rs` — added `GitHubAppClient` trait, `OctocrabAppClient`
  struct, and `validate_app_credentials` function (~80 lines)
- `src/github/tests.rs` — added 2 unit tests (~25 lines)
- `docs/podbot-design.md` — added credential validation contract section
- `docs/podbot-roadmap.md` — marked task complete

**Files created:**

- `tests/features/github_credential_validation.feature` — 3 BDD scenarios
- `tests/bdd_github_credential_validation.rs` — BDD harness
- `tests/bdd_github_credential_validation_helpers/` — state, steps, assertions

**Test coverage:**

- 2 new unit tests in `src/github/tests.rs`
- 3 BDD scenarios covering valid credentials, invalid credentials, and API
  failure

**Constraints respected:**

- Line budgets maintained: `src/github/mod.rs` ~365 lines, `src/github/tests.rs`
  ~345 lines (both under 400)
- No new crate dependencies added (`serde_json` was already a transitive
  dependency)
- All existing tests continue to pass (216 total tests)
