//! Unit tests for GitHub App installation-token acquisition.

use std::time::{Duration, SystemTime};

use rstest::{fixture, rstest};

use super::*;
use crate::github::{MockGitHubInstallationTokenClient, acquire_installation_token_with_client};

const FIXTURE_TOKEN: &str = "ghs_secret_fixture_token";
const INSTALLATION_ID: u64 = 42;

#[fixture]
fn acquired_at() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000)
}

#[fixture]
fn expiry_buffer() -> Duration {
    Duration::from_secs(300)
}

#[rstest]
fn token_metadata_uses_one_hour_lifetime(acquired_at: SystemTime, expiry_buffer: Duration) {
    let token =
        InstallationAccessToken::new(String::from(FIXTURE_TOKEN), acquired_at, expiry_buffer)
            .expect("token metadata should be representable");

    assert_eq!(token.token(), FIXTURE_TOKEN);
    assert_eq!(token.acquired_at(), acquired_at);
    assert_eq!(
        token.expires_at(),
        acquired_at + Duration::from_secs(60 * 60)
    );
}

#[rstest]
fn token_metadata_subtracts_refresh_buffer(acquired_at: SystemTime, expiry_buffer: Duration) {
    let token =
        InstallationAccessToken::new(String::from(FIXTURE_TOKEN), acquired_at, expiry_buffer)
            .expect("token metadata should be representable");

    assert_eq!(token.refresh_after(), token.expires_at() - expiry_buffer);
}

#[rstest]
fn token_debug_redacts_secret(acquired_at: SystemTime, expiry_buffer: Duration) {
    let token =
        InstallationAccessToken::new(String::from(FIXTURE_TOKEN), acquired_at, expiry_buffer)
            .expect("token metadata should be representable");

    let debug_output = format!("{token:?}");

    assert!(
        !debug_output.contains(FIXTURE_TOKEN),
        "debug output must not expose token: {debug_output}"
    );
    assert!(
        debug_output.contains("<redacted>"),
        "debug output should signpost redaction: {debug_output}"
    );
}

#[rstest]
fn log_fields_omit_secret(acquired_at: SystemTime, expiry_buffer: Duration) {
    let token =
        InstallationAccessToken::new(String::from(FIXTURE_TOKEN), acquired_at, expiry_buffer)
            .expect("token metadata should be representable");

    let fields = token.log_fields(INSTALLATION_ID, expiry_buffer);
    let debug_output = format!("{fields:?}");

    assert_eq!(fields.installation_id, INSTALLATION_ID);
    assert_eq!(fields.expiry_buffer, expiry_buffer);
    assert!(
        !debug_output.contains(FIXTURE_TOKEN),
        "log fields must not expose token: {debug_output}"
    );
}

#[rstest]
#[tokio::test]
async fn acquire_with_client_returns_token(acquired_at: SystemTime, expiry_buffer: Duration) {
    let token =
        InstallationAccessToken::new(String::from(FIXTURE_TOKEN), acquired_at, expiry_buffer)
            .expect("token metadata should be representable");
    let expected = token.clone();
    let expected_buffer = expiry_buffer;
    let mut mock = MockGitHubInstallationTokenClient::new();
    mock.expect_acquire_installation_token()
        .withf(move |installation_id, buffer| {
            *installation_id == INSTALLATION_ID && *buffer == expected_buffer
        })
        .times(1)
        .return_once(move |_, _| Box::pin(async move { Ok(token) }));

    let result =
        acquire_installation_token_with_client(&mock, INSTALLATION_ID, expiry_buffer).await;

    assert_eq!(result.expect("mocked acquisition should succeed"), expected);
}

#[rstest]
#[tokio::test]
async fn acquire_with_client_maps_semantic_failure(expiry_buffer: Duration) {
    let mut mock = MockGitHubInstallationTokenClient::new();
    mock.expect_acquire_installation_token()
        .times(1)
        .returning(|_, _| {
            Box::pin(async {
                Err(GitHubError::TokenAcquisitionFailed {
                    message: String::from("installation suspended"),
                })
            })
        });

    let result =
        acquire_installation_token_with_client(&mock, INSTALLATION_ID, expiry_buffer).await;

    match result {
        Err(GitHubError::TokenAcquisitionFailed { message }) => {
            assert!(message.contains("installation suspended"));
            assert!(
                !message.contains(FIXTURE_TOKEN),
                "error message must not expose token: {message}"
            );
        }
        other => panic!("expected token acquisition failure, got: {other:?}"),
    }
}
