//! Unit tests for installation-token acquisition and expiry policy.

use std::time::Duration;

use chrono::{DateTime, Utc};
use octocrab::models::InstallationToken;
use rstest::{fixture, rstest};
use serde_json::json;

use super::installation_token::{
    InstallationAccessToken, MockGitHubInstallationTokenClient,
    installation_token_with_client_and_time, token_from_response,
};
use super::*;

#[fixture]
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-04-22T12:00:00Z")
        .expect("fixture timestamp should parse")
        .with_timezone(&Utc)
}

#[fixture]
fn future_expiry() -> &'static str {
    "2026-04-22T12:10:00Z"
}

#[fixture]
fn near_expiry() -> &'static str {
    "2026-04-22T12:02:00Z"
}

fn assert_token_expired(result: &Result<InstallationAccessToken, GitHubError>) {
    assert!(
        matches!(result, Err(GitHubError::TokenExpired)),
        "expected TokenExpired, got: {result:?}"
    );
}

fn test_installation_token(token: &str, expires_at: Option<&str>) -> InstallationToken {
    serde_json::from_value(json!({
        "token": token,
        "expires_at": expires_at,
        "permissions": {},
        "repositories": null,
    }))
    .expect("test installation token JSON should deserialize")
}

#[rstest]
fn token_response_beyond_buffer_succeeds(now: DateTime<Utc>, future_expiry: &str) {
    let token = test_installation_token("ghs_valid", Some(future_expiry));
    let result = token_from_response(token, Duration::from_secs(300), now);
    let access_token = result.expect("token should remain valid outside the buffer");
    assert_eq!(access_token.token(), "ghs_valid");
    assert_eq!(
        access_token.expires_at().to_rfc3339(),
        "2026-04-22T12:10:00+00:00"
    );
}

#[rstest]
fn token_response_inside_buffer_returns_token_expired(now: DateTime<Utc>, near_expiry: &str) {
    let token = test_installation_token("ghs_near", Some(near_expiry));
    let result = token_from_response(token, Duration::from_secs(300), now);
    assert_token_expired(&result);
}

#[rstest]
fn missing_expiry_metadata_returns_deterministic_error(now: DateTime<Utc>) {
    let token = test_installation_token("ghs_missing_expiry", None);
    let result = token_from_response(token, Duration::from_secs(300), now);
    match result {
        Err(GitHubError::TokenAcquisitionFailed { message }) => {
            assert!(
                message.contains("did not include expires_at"),
                "message should mention missing expiry metadata: {message}"
            );
        }
        other => panic!("expected TokenAcquisitionFailed, got: {other:?}"),
    }
}

#[rstest]
fn malformed_expiry_metadata_returns_deterministic_error(now: DateTime<Utc>) {
    let token = test_installation_token("ghs_bad_expiry", Some("not-a-timestamp"));
    let result = token_from_response(token, Duration::from_secs(300), now);
    match result {
        Err(GitHubError::TokenAcquisitionFailed { message }) => {
            assert!(
                message.contains("invalid installation token expiry timestamp"),
                "message should mention invalid expiry metadata: {message}"
            );
        }
        other => panic!("expected TokenAcquisitionFailed, got: {other:?}"),
    }
}

#[rstest]
fn access_token_debug_redacts_secret(now: DateTime<Utc>, future_expiry: &str) {
    let token = test_installation_token("ghs_debug_redaction", Some(future_expiry));
    let access_token = token_from_response(token, Duration::from_secs(300), now)
        .expect("token should parse and remain valid");
    let debug = format!("{access_token:?}");
    assert!(
        !debug.contains("ghs_debug_redaction"),
        "debug output should redact the token secret: {debug}"
    );
    assert!(
        debug.contains("[REDACTED]"),
        "debug output should include an explicit redaction marker: {debug}"
    );
}

#[rstest]
#[tokio::test]
async fn installation_token_with_client_maps_mock_error() {
    let mut mock = MockGitHubInstallationTokenClient::new();
    mock.expect_acquire_installation_token()
        .times(1)
        .with(mockall::predicate::eq(77_u64))
        .returning(|_| {
            Box::pin(async {
                Err(GitHubError::TokenAcquisitionFailed {
                    message: String::from("GitHub installation not found (HTTP 404)."),
                })
            })
        });

    let now = DateTime::parse_from_rfc3339("2026-04-22T12:00:00Z")
        .expect("fixture timestamp should parse")
        .with_timezone(&Utc);
    let result =
        installation_token_with_client_and_time(&mock, 77, Duration::from_secs(300), now).await;

    match result {
        Err(GitHubError::TokenAcquisitionFailed { message }) => {
            assert!(message.contains("installation not found"));
        }
        other => panic!("expected TokenAcquisitionFailed, got: {other:?}"),
    }
}
