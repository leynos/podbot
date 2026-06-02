//! Unit tests for Octocrab retry metrics and warning emission.

use std::sync::{Arc, Mutex};

use http::{Method, Request, StatusCode};
use metrics::{
    Counter, CounterFn, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
};
use octocrab::service::middleware::retry::RateLimitMetrics;

use super::PodbotOctocrabRetryMetrics;

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

fn github_request() -> Request<octocrab::OctoBody> {
    match Request::builder()
        .method(Method::GET)
        .uri("https://api.github.com/repos/leynos/podbot")
        .body(octocrab::OctoBody::empty())
    {
        Ok(request) => request,
        Err(error) => panic!("request should build: {error}"),
    }
}

fn capture_retry_logs(run_test: impl FnOnce()) -> String {
    let buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer = SharedLogBuffer {
        buffer: Arc::clone(&buffer),
    };
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .without_time()
        .with_ansi(false)
        .with_writer(writer)
        .finish();

    tracing::subscriber::with_default(subscriber, run_test);

    let bytes = match buffer.lock() {
        Ok(logs) => logs.clone(),
        Err(error) => panic!("log buffer lock should not be poisoned: {error}"),
    };
    match String::from_utf8(bytes) {
        Ok(logs) => logs,
        Err(error) => panic!("logs should be UTF-8: {error}"),
    }
}

#[derive(Clone)]
struct SharedLogBuffer {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for SharedLogBuffer {
    type Writer = SharedLogWriter;

    fn make_writer(&'writer self) -> Self::Writer {
        SharedLogWriter {
            buffer: Arc::clone(&self.buffer),
        }
    }
}

struct SharedLogWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl std::io::Write for SharedLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer
            .lock()
            .map_err(|error| std::io::Error::other(format!("log buffer poisoned: {error}")))?
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn retry_after_error_logs_and_records_retryable_response_metric() {
    let recorder = RecordingMetrics::default();
    let request = github_request();
    let logs = metrics::with_local_recorder(&recorder, || {
        capture_retry_logs(|| {
            PodbotOctocrabRetryMetrics.retry_after_error(&request, StatusCode::BAD_GATEWAY, 2);
        })
    });

    assert!(
        logs.contains("Octocrab retry policy observed a retryable GitHub API response"),
        "retry warning should include retryable response message: {logs}"
    );
    assert!(
        logs.contains("retries_remaining=2"),
        "retry warning should include remaining retries: {logs}"
    );
    assert!(
        logs.contains("status_code=502"),
        "retry warning should include status code: {logs}"
    );
    assert!(
        !logs.contains("waiting_seconds"),
        "retryable response warning should not include wait duration: {logs}"
    );

    assert_eq!(
        recorder.events(),
        vec![CounterEvent {
            name: "podbot.github.octocrab.retry.events.total".to_owned(),
            labels: vec![
                ("operation".to_owned(), "github_api".to_owned()),
                ("event".to_owned(), "retryable_response".to_owned()),
                ("status_class".to_owned(), "5xx".to_owned()),
            ],
            value: 1,
        }]
    );
}

#[test]
fn rate_limited_logs_wait_duration_and_records_rate_limit_metric() {
    let recorder = RecordingMetrics::default();
    let request = github_request();
    let logs = metrics::with_local_recorder(&recorder, || {
        capture_retry_logs(|| {
            PodbotOctocrabRetryMetrics.rate_limited(&request, StatusCode::TOO_MANY_REQUESTS, 1, 30);
        })
    });

    assert!(
        logs.contains("Octocrab retry policy is waiting before retrying a GitHub API request"),
        "rate-limit warning should include wait message: {logs}"
    );
    assert!(
        logs.contains("waiting_seconds=30"),
        "rate-limit warning should include wait duration: {logs}"
    );

    assert_eq!(
        recorder.events(),
        vec![CounterEvent {
            name: "podbot.github.octocrab.retry.events.total".to_owned(),
            labels: vec![
                ("operation".to_owned(), "github_api".to_owned()),
                ("event".to_owned(), "rate_limited".to_owned()),
                ("status_class".to_owned(), "4xx".to_owned()),
            ],
            value: 1,
        }]
    );
}
