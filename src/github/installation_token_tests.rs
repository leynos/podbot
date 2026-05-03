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

const FIXTURE_RSA_PRIVATE_KEY: &str = include_str!("../../tests/fixtures/test_rsa_private_key.pem");

#[fixture]
fn now() -> Result<DateTime<Utc>, chrono::ParseError> {
    Ok(DateTime::parse_from_rfc3339("2026-04-22T12:00:00Z")?.with_timezone(&Utc))
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

fn assert_acquisition_failed(
    result: &Result<InstallationAccessToken, GitHubError>,
    fragment: &str,
) {
    match result {
        Err(GitHubError::TokenAcquisitionFailed { message }) => {
            assert!(
                message.contains(fragment),
                "message should contain {fragment:?}: {message}"
            );
        }
        other => panic!("expected TokenAcquisitionFailed, got: {other:?}"),
    }
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
fn token_response_beyond_buffer_succeeds(
    now: Result<DateTime<Utc>, chrono::ParseError>,
    future_expiry: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let current_time = now?;
    let token = test_installation_token("ghs_valid", Some(future_expiry));
    let result = token_from_response(token, Duration::from_secs(300), current_time);
    let access_token = result.expect("token should remain valid outside the buffer");
    if access_token.token() != "ghs_valid" {
        return Err(format!("unexpected token: {}", access_token.token()).into());
    }
    let expires_at = access_token.expires_at().to_rfc3339();
    if expires_at != "2026-04-22T12:10:00+00:00" {
        return Err(format!("unexpected expiry: {expires_at}").into());
    }
    Ok(())
}

#[rstest]
fn token_response_inside_buffer_returns_token_expired(
    now: Result<DateTime<Utc>, chrono::ParseError>,
    near_expiry: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let current_time = now?;
    let token = test_installation_token("ghs_near", Some(near_expiry));
    let result = token_from_response(token, Duration::from_secs(300), current_time);
    assert_token_expired(&result);
    Ok(())
}

#[rstest]
#[case(None, "did not include expires_at")]
#[case(Some("not-a-timestamp"), "invalid installation token expiry timestamp")]
fn bad_expiry_metadata_returns_deterministic_error(
    now: Result<DateTime<Utc>, chrono::ParseError>,
    #[case] expires_at: Option<&str>,
    #[case] expected_fragment: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let current_time = now?;
    let token = test_installation_token("ghs_bad_expiry", expires_at);
    let result = token_from_response(token, Duration::from_secs(300), current_time);
    assert_acquisition_failed(&result, expected_fragment);
    Ok(())
}

#[rstest]
fn access_token_debug_redacts_secret(
    now: Result<DateTime<Utc>, chrono::ParseError>,
    future_expiry: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let current_time = now?;
    let token = test_installation_token("ghs_debug_redaction", Some(future_expiry));
    let access_token = token_from_response(token, Duration::from_secs(300), current_time)
        .expect("token should parse and remain valid");
    let debug = format!("{access_token:?}");
    if debug.contains("ghs_debug_redaction") {
        return Err(format!("debug output should redact the token secret: {debug}").into());
    }
    if !debug.contains("[REDACTED]") {
        return Err(format!("debug output should include redaction marker: {debug}").into());
    }
    Ok(())
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

#[rstest]
#[tokio::test]
async fn installation_token_with_factory_returns_valid_token(
    now: Result<DateTime<Utc>, chrono::ParseError>,
    future_expiry: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use camino::Utf8PathBuf;
    use std::fs;
    use tempfile::tempdir;

    let current_time = now?;
    let temp = tempdir()?;
    let key_path = Utf8PathBuf::from_path_buf(temp.path().join("test.pem"))
        .expect("temp path should be UTF-8");
    fs::write(&key_path, FIXTURE_RSA_PRIVATE_KEY)?;

    let expected_expiry = future_expiry.to_owned();
    let expected_expiry_clone = expected_expiry.clone();
    let request = InstallationTokenRequest::new(42, 77, &key_path, Duration::from_secs(300))
        .with_now(current_time);

    let result = installation_token_with_factory(request, |_app_id, _key| {
        let mut mock = MockGitHubInstallationTokenClient::new();
        let expiry = expected_expiry_clone.clone();
        mock.expect_acquire_installation_token()
            .times(1)
            .returning(move |_| {
                let token = serde_json::from_value(json!({
                    "token": "ghs_via_buffer",
                    "expires_at": expiry,
                    "permissions": {},
                    "repositories": null,
                }))
                .expect("should deserialize");
                Box::pin(async move { Ok(token) })
            });
        Ok(mock)
    })
    .await?;

    if result.token() != "ghs_via_buffer" {
        return Err(format!("unexpected token: {}", result.token()).into());
    }

    let expected = chrono::DateTime::parse_from_rfc3339(&expected_expiry)?
        .with_timezone(&Utc)
        .to_rfc3339();
    let actual = result.expires_at().to_rfc3339();
    if expected != actual {
        return Err(format!("unexpected expiry: {actual}").into());
    }

    Ok(())
}
