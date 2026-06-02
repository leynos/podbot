//! Unit tests for GitHub App installation-token acquisition.

use std::time::{Duration, SystemTime};

use rstest::{fixture, rstest};
use snafu::GenerateImplicitData;

use super::*;
use crate::github::test_support::{CounterEvent, RecordingMetrics};
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

#[rstest]
#[case::expiry_before_acquisition(-1_i64, 300_u64, "expiry time precedes acquisition time")]
#[case::refresh_before_acquisition(60_i64, 120_u64, "refresh time precedes acquisition time")]
fn token_metadata_rejects_invalid_ordering(
    acquired_at: SystemTime,
    #[case] expires_offset_secs: i64,
    #[case] expiry_buffer_secs: u64,
    #[case] expected_fragment: &'static str,
) {
    let expires_at = if expires_offset_secs >= 0 {
        acquired_at + Duration::from_secs(expires_offset_secs.unsigned_abs())
    } else {
        acquired_at - Duration::from_secs(expires_offset_secs.unsigned_abs())
    };
    let expiry_buffer = Duration::from_secs(expiry_buffer_secs);
    assert_metadata_rejects(acquired_at, expires_at, expiry_buffer, expected_fragment);
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

#[rstest]
#[case::timed_out(std::io::ErrorKind::TimedOut, true)]
#[case::connection_refused(std::io::ErrorKind::ConnectionRefused, false)]
fn token_error_timeout_detection_checks_service_io_errors(
    #[case] error_kind: std::io::ErrorKind,
    #[case] expected_timeout: bool,
) {
    let io_error = std::io::Error::new(error_kind, "transport failure");
    let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(io_error);
    let error = octocrab::Error::Service {
        source: boxed,
        backtrace: snafu::Backtrace::generate(),
    };

    assert_eq!(is_timeout_error(&error), expected_timeout);
}

#[rstest]
#[case::success("success")]
#[case::failure("failure")]
fn record_token_acquisition_metrics_emits_counter_and_histogram(#[case] status: &'static str) {
    let recorder = RecordingMetrics::default();
    metrics::with_local_recorder(&recorder, || {
        record_token_acquisition_metrics(status, Duration::from_millis(42));
    });

    assert_eq!(
        recorder.events(),
        vec![CounterEvent {
            name: "podbot.github.installation_token.acquisitions.total".to_owned(),
            labels: vec![
                ("operation".to_owned(), "installation_token".to_owned()),
                ("status".to_owned(), status.to_owned()),
            ],
            value: 1,
        }],
        "acquisitions counter should be incremented once for status={status}"
    );

    let histogram_events = recorder.histogram_events();
    assert_eq!(
        histogram_events.len(),
        1,
        "exactly one latency histogram observation expected for status={status}"
    );
    let hist = histogram_events
        .first()
        .expect("histogram event count checked above");
    assert_eq!(
        hist.name,
        "podbot.github.installation_token.latency_seconds"
    );
    assert!(
        hist.labels
            .contains(&("operation".to_owned(), "installation_token".to_owned())),
        "latency histogram should have operation label: {hist:?}"
    );
    assert!(
        hist.labels
            .contains(&("status".to_owned(), status.to_owned())),
        "latency histogram should have status={status} label: {hist:?}"
    );
    assert!(
        hist.value >= 0.0,
        "latency histogram value should be non-negative: {}",
        hist.value
    );
}

#[test]
fn warn_token_acquisition_failure_increments_timeout_counter_for_timed_out_errors() {
    let recorder = RecordingMetrics::default();
    let io_error = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
    let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(io_error);
    let error = octocrab::Error::Service {
        source: boxed,
        backtrace: snafu::Backtrace::generate(),
    };

    metrics::with_local_recorder(&recorder, || {
        warn_token_acquisition_failure(
            INSTALLATION_ID,
            Duration::from_secs(300),
            Duration::from_millis(5_000),
            &error,
        );
    });

    let events = recorder.events();
    let timeout_counter = events
        .iter()
        .find(|e| e.name == "podbot.github.installation_token.timeout_failures.total");
    assert!(
        timeout_counter.is_some(),
        "expected timeout counter for TimedOut error: {events:?}"
    );
    assert_eq!(
        timeout_counter
            .expect("timeout counter checked above")
            .value,
        1,
        "timeout counter should be incremented once"
    );
}

#[test]
fn warn_token_acquisition_failure_does_not_increment_timeout_counter_for_connection_refused() {
    let recorder = RecordingMetrics::default();
    let io_error = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
    let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(io_error);
    let error = octocrab::Error::Service {
        source: boxed,
        backtrace: snafu::Backtrace::generate(),
    };

    metrics::with_local_recorder(&recorder, || {
        warn_token_acquisition_failure(
            INSTALLATION_ID,
            Duration::from_secs(300),
            Duration::from_millis(100),
            &error,
        );
    });

    let events = recorder.events();
    let timeout_counter = events
        .iter()
        .find(|e| e.name == "podbot.github.installation_token.timeout_failures.total");
    assert!(
        timeout_counter.is_none(),
        "timeout counter must not be emitted for non-timeout errors: {events:?}"
    );
}
