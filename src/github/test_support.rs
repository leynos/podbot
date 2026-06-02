//! Shared test-support types for GitHub retry metrics.
//!
//! Provides `CounterEvent`, `RecordingMetrics`, and `RecordedCounter` so
//! unit tests and integration tests can use the same metrics recorder
//! implementation without duplicating the types.

#![cfg(any(test, feature = "internal"))]

use std::sync::{Arc, Mutex};

use metrics::{
    Counter, CounterFn, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CounterEvent {
    pub name: String,
    pub labels: Vec<(String, String)>,
    pub value: u64,
}

#[derive(Clone, Default)]
pub struct RecordingMetrics {
    events: Arc<Mutex<Vec<CounterEvent>>>,
}

impl RecordingMetrics {
    #[must_use]
    pub fn events(&self) -> Vec<CounterEvent> {
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
