//! Unit tests for GitHub API error classification.
//!
//! Covers the `classify_by_status` function, verifying that HTTP status
//! codes from the GitHub API are classified into actionable error messages
//! with remediation hints. Also covers the non-HTTP (network) fallback
//! path through `classify_github_api_error`.

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
fn classify_5xx_error_mentions_api_unavailable() {
    let msg = classify_by_status(503, "Service temporarily unavailable", "full error text");
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
fn classify_500_error_mentions_api_unavailable() {
    let msg = classify_by_status(500, "Internal Server Error", "full error text");
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

// ── classify_github_api_error integration (non-HTTP path) ─────────────

#[rstest]
fn classify_network_error_mentions_connectivity() {
    // Use the Service variant to simulate a non-HTTP (network) failure.
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
