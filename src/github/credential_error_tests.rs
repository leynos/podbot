//! Unit tests for GitHub API error classification.
//!
//! Covers the `classify_by_status` function, verifying that HTTP status
//! codes from the GitHub API are classified into actionable error messages
//! with remediation hints. Also covers the non-HTTP (network) fallback
//! path through `classify_github_api_error`.
//!
//! # Integration testing note
//!
//! Direct unit tests for `classify_github_api_error` with HTTP status-based
//! `octocrab::Error::GitHub` variants are not included because:
//! - `octocrab::error::GitHubError` is `#[non_exhaustive]` with no public
//!   constructor or `Deserialize` impl, so it cannot be constructed outside
//!   the octocrab crate
//! - The `octocrab::Error::Http` variant wraps `http::Error` (HTTP protocol
//!   errors such as malformed requests), **not** HTTP response errors with
//!   status codes — it falls into the non-HTTP fallback path, same as
//!   `Service`
//! - Adding HTTP mocking (wiremock/mockito) would add significant complexity
//!   for tests that are already covered by the BDD integration tests
//!
//! The HTTP error classification path is validated through:
//! 1. Unit tests of `classify_by_status` (this file) — tests the
//!    classification logic for all status codes including rate-limit 403
//! 2. BDD tests (`tests/bdd_github_credential_errors.rs`) — exercises the
//!    full integration path with mocked GitHub API responses

use super::*;
use rstest::rstest;
use snafu::GenerateImplicitData;

// ── classify_by_status tests ──────────────────────────────────────────

#[rstest]
fn classify_401_error_mentions_credentials_rejected() {
    let msg = classify_by_status(401, "Bad credentials", "full error text");
    assert!(
        msg.contains("credentials rejected"),
        "expected 'credentials rejected' in: {msg}"
    );
    assert!(
        msg.contains("regenerate the private key"),
        "expected regeneration hint in: {msg}"
    );
    assert!(msg.contains("clock"), "expected clock-skew hint in: {msg}");
}

#[rstest]
fn classify_403_error_mentions_insufficient_permissions() {
    let msg = classify_by_status(403, "Resource not accessible", "full error text");
    assert!(
        msg.contains("insufficient permissions"),
        "expected 'insufficient permissions' in: {msg}"
    );
    assert!(
        msg.contains("permission settings"),
        "expected permission settings hint in: {msg}"
    );
}

// ── rate-limit 403 tests ────────────────────────────────────────────

#[rstest]
#[case("API rate limit exceeded for installation ID 12345")]
#[case("You have exceeded a secondary rate limit")]
fn classify_403_rate_limit_mentions_rate_limit(#[case] body: &str) {
    let msg = classify_by_status(403, body, "full error text");
    assert!(
        msg.contains("rate limit exceeded"),
        "expected 'rate limit exceeded' in: {msg}"
    );
    assert!(msg.contains("Wait"), "expected retry hint in: {msg}");
    assert!(
        !msg.contains("insufficient permissions"),
        "rate-limit 403 should not mention permissions: {msg}"
    );
}

#[rstest]
fn classify_403_rate_limit_preserves_raw_message() {
    let raw = "API rate limit exceeded for installation ID 12345";
    let msg = classify_by_status(403, raw, "full error text");
    assert!(
        msg.contains(raw),
        "expected raw message '{raw}' preserved in: {msg}"
    );
}

#[rstest]
fn classify_404_error_mentions_app_not_found() {
    let msg = classify_by_status(404, "Not Found", "full error text");
    assert!(msg.contains("not found"), "expected 'not found' in: {msg}");
    assert!(
        msg.contains("github.app_id"),
        "expected app_id verification hint in: {msg}"
    );
}

#[rstest]
#[case(500, "Internal Server Error")]
#[case(503, "Service temporarily unavailable")]
fn classify_5xx_error_mentions_api_unavailable(#[case] status: u16, #[case] body: &str) {
    let msg = classify_by_status(status, body, "full error text");
    assert!(
        msg.contains("unavailable"),
        "expected 'unavailable' in: {msg}"
    );
    assert!(
        msg.contains("githubstatus.com"),
        "expected status page hint in: {msg}"
    );
}

#[rstest]
fn classified_error_preserves_raw_message() {
    let raw = "Bad credentials";
    let msg = classify_by_status(401, raw, "full error text");
    assert!(
        msg.contains(raw),
        "expected raw message '{raw}' preserved in: {msg}"
    );
}

#[rstest]
fn classify_unexpected_status_includes_code() {
    let msg = classify_by_status(418, "I'm a teapot", "full error text 418");
    assert!(
        msg.contains("unexpected response"),
        "expected 'unexpected response' in: {msg}"
    );
    assert!(msg.contains("418"), "expected status code 418 in: {msg}");
}

// ── classify_github_api_error (non-HTTP path) ─────────────────────────

#[rstest]
fn classify_network_error_mentions_connectivity() {
    // Use the Service variant to simulate a non-HTTP (network) failure.
    // This exercises the fallback path in classify_github_api_error when
    // the error is not a GitHub API error with a status code.
    //
    // Note: While octocrab::Error is #[non_exhaustive] and this test
    // directly constructs the Service variant with snafu::Backtrace::generate(),
    // this is acceptable because:
    // 1. Service is a documented public variant of the Error enum
    // 2. The backtrace field is required by snafu and expected to be present
    // 3. Creating a real network error would require HTTP mocking infrastructure
    //    (wiremock/mockito), adding significant complexity for a simple test
    // 4. If octocrab changes its error structure, the compilation will fail,
    //    alerting us to update the test
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused");
    let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(io_err);
    let error = octocrab::Error::Service {
        source: boxed,
        backtrace: snafu::Backtrace::generate(),
    };
    let classified = classify_github_api_error(error);
    let message = classified.to_string();
    assert!(
        message.contains("connectivity") || message.contains("network"),
        "expected connectivity/network hint in: {message}"
    );
}
