//! Unit tests for GitHub App installation-token acquisition.

use std::time::{Duration, SystemTime};

use proptest::prelude::*;
use rstest::{fixture, rstest};
use snafu::GenerateImplicitData;

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

/// Asserts that [`InstallationAccessToken::from_metadata`] returns a
/// [`GitHubError::TokenAcquisitionFailed`] whose message contains `expected_fragment`.
fn assert_metadata_rejects(
    acquired_at: SystemTime,
    expires_at: SystemTime,
    expiry_buffer: Duration,
    expected_fragment: &str,
) {
    let result = InstallationAccessToken::from_metadata(
        String::from(FIXTURE_TOKEN),
        acquired_at,
        expires_at,
        expiry_buffer,
    );
    match result {
        Err(GitHubError::TokenAcquisitionFailed { message }) => {
            assert!(
                message.contains(expected_fragment),
                "expected message to contain {expected_fragment:?}, got: {message}"
            );
        }
        other => panic!("expected token metadata failure, got: {other:?}"),
    }
}

#[derive(Clone, Copy, Debug)]
enum RejectionVariant {
    ExpiryBeforeAcquisition,
    RefreshBeforeAcquisition,
}

#[rstest]
#[case(RejectionVariant::ExpiryBeforeAcquisition)]
#[case(RejectionVariant::RefreshBeforeAcquisition)]
fn token_metadata_rejects_invalid_metadata(
    acquired_at: SystemTime,
    expiry_buffer: Duration,
    #[case] variant: RejectionVariant,
) {
    let (expires_at, buffer, expected_fragment) = match variant {
        RejectionVariant::ExpiryBeforeAcquisition => (
            acquired_at - Duration::from_secs(1),
            expiry_buffer,
            "expiry time precedes acquisition time",
        ),
        RejectionVariant::RefreshBeforeAcquisition => (
            acquired_at + Duration::from_secs(60),
            Duration::from_secs(120),
            "refresh time precedes acquisition time",
        ),
    };
    assert_metadata_rejects(acquired_at, expires_at, buffer, expected_fragment);
}

proptest! {
    #[test]
    fn token_metadata_refresh_never_precedes_acquisition(
        lifetime_secs in 0_u64..=7_200,
        buffer_secs in 0_u64..=7_200,
    ) {
        let acquired_at = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let expires_at = acquired_at + Duration::from_secs(lifetime_secs);
        let expiry_buffer = Duration::from_secs(buffer_secs);

        let result = InstallationAccessToken::from_metadata(
            String::from(FIXTURE_TOKEN),
            acquired_at,
            expires_at,
            expiry_buffer,
        );

        if buffer_secs <= lifetime_secs {
            match result {
                Ok(token) => {
                    prop_assert!(token.refresh_after().duration_since(acquired_at).is_ok());
                }
                Err(error) => {
                    prop_assert!(
                        false,
                        "expected internally consistent metadata, got {error}"
                    );
                }
            }
        } else {
            let is_error = matches!(
                result,
                Err(GitHubError::TokenAcquisitionFailed { .. })
            );
            prop_assert!(is_error);
        }
    }
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

#[rstest]
fn token_error_mapping_preserves_transport_failure_context() {
    let io_error = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused");
    let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(io_error);
    let error = octocrab::Error::Service {
        source: boxed,
        backtrace: snafu::Backtrace::generate(),
    };

    let classified = classify_token_error(error);

    match classified {
        GitHubError::TokenAcquisitionFailed { message } => {
            assert!(
                message.contains("connectivity") || message.contains("network"),
                "expected transport remediation context in: {message}"
            );
            assert!(
                !message.contains("GitHub rejected installation token acquisition"),
                "transport failures must not be reported as GitHub rejections: {message}"
            );
        }
        other => panic!("expected token acquisition failure, got: {other:?}"),
    }
}
