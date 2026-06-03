//! Shared test-support types for GitHub metrics.
//!
//! Provides `CounterEvent`, `HistogramEvent`, `RecordingMetrics`,
//! `RecordedCounter`, and `RecordedHistogram` so unit tests and integration
//! tests can use the same metrics recorder implementation without
//! duplicating the types.

#![cfg(any(test, feature = "internal"))]

use std::sync::{Arc, Mutex};

use metrics::{
    Counter, CounterFn, Gauge, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CounterEvent {
    pub name: String,
    pub labels: Vec<(String, String)>,
    pub value: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistogramEvent {
    pub name: String,
    pub labels: Vec<(String, String)>,
    pub value: f64,
}

#[derive(Clone, Default)]
pub struct RecordingMetrics {
    events: Arc<Mutex<Vec<CounterEvent>>>,
    histogram_events: Arc<Mutex<Vec<HistogramEvent>>>,
}

impl RecordingMetrics {
    #[must_use]
    pub fn events(&self) -> Vec<CounterEvent> {
        match self.events.lock() {
            Ok(events) => events.clone(),
            Err(error) => panic!("metrics events lock should not be poisoned: {error}"),
        }
    }

    #[must_use]
    pub fn histogram_events(&self) -> Vec<HistogramEvent> {
        match self.histogram_events.lock() {
            Ok(events) => events.clone(),
            Err(error) => panic!("histogram events lock should not be poisoned: {error}"),
        }
    }
}

fn extract_key_labels(key: &Key) -> Vec<(String, String)> {
    key.labels()
        .map(|label| (label.key().to_owned(), label.value().to_owned()))
        .collect()
}

impl Recorder for RecordingMetrics {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        Counter::from_arc(Arc::new(RecordedCounter {
            event: CounterEvent {
                name: key.name().to_owned(),
                labels: extract_key_labels(key),
                value: 0,
            },
            events: Arc::clone(&self.events),
        }))
    }

    fn register_gauge(&self, _key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        Gauge::noop()
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::from_arc(Arc::new(RecordedHistogram {
            event: HistogramEvent {
                name: key.name().to_owned(),
                labels: extract_key_labels(key),
                value: 0.0,
            },
            events: Arc::clone(&self.histogram_events),
        }))
    }
}

pub struct RecordedCounter {
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

pub struct RecordedHistogram {
    event: HistogramEvent,
    events: Arc<Mutex<Vec<HistogramEvent>>>,
}

impl HistogramFn for RecordedHistogram {
    fn record(&self, value: f64) {
        let mut event = self.event.clone();
        event.value = value;
        match self.events.lock() {
            Ok(mut events) => events.push(event),
            Err(error) => panic!("histogram events lock should not be poisoned: {error}"),
        }
    }
}
