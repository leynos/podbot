//! Integration tests for GitHub retry metric recording.
//!
//! Verifies that [`podbot::github::test_record_octocrab_retry_event`] emits
//! `podbot.github.octocrab.retry.events.total` counter events with the
//! correct `operation`, `event`, and `status_class` labels when called
//! through the public test seam exposed under `--features internal`.
//!
//! These tests complement the unit tests in
//! `src/github/retry_metrics_tests.rs` by exercising the crate's public
//! module boundary and asserting that [`podbot::github::test_support`]
//! types are accessible and behave as documented.
//!
//! Requires `--features internal` to compile; guarded by
//! `#![cfg(feature = "internal")]`.
//!
//! See also: `src/github/retry_metrics.rs`,
//! `src/github/retry_metrics_tests.rs`.

#![cfg(feature = "internal")]

use http::StatusCode;

use podbot::github::test_support::{CounterEvent, RecordingMetrics};

#[test]
fn github_retry_metrics_record_status_class_labels() {
    let recorder = RecordingMetrics::default();

    metrics::with_local_recorder(&recorder, || {
        podbot::github::test_record_octocrab_retry_event(
            "retryable_response",
            StatusCode::SERVICE_UNAVAILABLE,
        );
        podbot::github::test_record_octocrab_retry_event(
            "rate_limited",
            StatusCode::TOO_MANY_REQUESTS,
        );
    });

    assert_eq!(
        recorder.events(),
        vec![
            CounterEvent {
                name: "podbot.github.octocrab.retry.events.total".to_owned(),
                labels: vec![
                    ("operation".to_owned(), "github_api".to_owned()),
                    ("event".to_owned(), "retryable_response".to_owned()),
                    ("status_class".to_owned(), "5xx".to_owned()),
                ],
                value: 1,
            },
            CounterEvent {
                name: "podbot.github.octocrab.retry.events.total".to_owned(),
                labels: vec![
                    ("operation".to_owned(), "github_api".to_owned()),
                    ("event".to_owned(), "rate_limited".to_owned()),
                    ("status_class".to_owned(), "4xx".to_owned()),
                ],
                value: 1,
            },
        ]
    );
}
