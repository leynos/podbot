//! Integration tests for GitHub retry metric recording.

#![cfg(feature = "internal")]

use std::sync::{Arc, Mutex};

use http::StatusCode;
use metrics::{
    Counter, CounterFn, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct CounterEvent {
    name: String,
    labels: Vec<(String, String)>,
    value: u64,
}

#[derive(Clone, Default)]
struct RecordingMetrics {
    events: Arc<Mutex<Vec<CounterEvent>>>,
}

impl RecordingMetrics {
    fn events(&self) -> Vec<CounterEvent> {
        match self.events.lock() {
            Ok(events) => events.clone(),
            Err(error) => panic!("metrics events lock should not be poisoned: {error}"),
        }
    }
}

impl Recorder for RecordingMetrics {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        Counter::from_arc(Arc::new(RecordedCounter {
            event: CounterEvent {
                name: key.name().to_owned(),
                labels: key
                    .labels()
                    .map(|label| (label.key().to_owned(), label.value().to_owned()))
                    .collect(),
                value: 0,
            },
            events: Arc::clone(&self.events),
        }))
    }

    fn register_gauge(&self, _key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        Gauge::noop()
    }

    fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::noop()
    }
}

struct RecordedCounter {
    event: CounterEvent,
    events: Arc<Mutex<Vec<CounterEvent>>>,
}

impl CounterFn for RecordedCounter {
    fn increment(&self, value: u64) {
        let mut event = self.event.clone();
        event.value = value;
        match self.events.lock() {
            Ok(mut events) => events.push(event),
            Err(error) => panic!("metrics events lock should not be poisoned: {error}"),
        }
    }

    fn absolute(&self, value: u64) {
        self.increment(value);
    }
}

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
