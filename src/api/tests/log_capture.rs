//! Log capture helpers for experimental API tests.
//!
//! These helpers keep the main API test module focused on behaviours while
//! centralizing the small tracing subscriber used to assert diagnostic context.

use std::sync::{Arc, Mutex};

use crate::api::CommandOutcome;

#[derive(Clone)]
struct SharedLogWriter {
    output: Arc<Mutex<Vec<u8>>>,
}

struct SharedLogBuffer {
    output: Arc<Mutex<Vec<u8>>>,
}

impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for SharedLogWriter {
    type Writer = SharedLogBuffer;

    fn make_writer(&'writer self) -> Self::Writer {
        SharedLogBuffer {
            output: Arc::clone(&self.output),
        }
    }
}

impl std::io::Write for SharedLogBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut output = self
            .output
            .lock()
            .map_err(|_| std::io::Error::other("log buffer mutex poisoned"))?;
        output.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Capture warning-level logs emitted synchronously by `operation`.
///
/// This helper installs a scoped default subscriber for the current test
/// thread. Use it only for same-thread log assertions; spawned work needs its
/// own subscriber plumbing.
pub(crate) fn capture_warning_logs(
    operation: impl FnOnce(),
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    capture_logs(operation, tracing::Level::WARN)
}

/// Capture logs up to `max_level` emitted synchronously by `operation`.
///
/// The subscriber is scoped with `tracing::subscriber::with_default`, so this
/// helper is intended for single-threaded test sections. It does not guarantee
/// capture from work moved onto other threads.
pub(crate) fn capture_logs(
    operation: impl FnOnce(),
    max_level: tracing::Level,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let output = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedLogWriter {
        output: Arc::clone(&output),
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .with_max_level(max_level)
        .finish();

    tracing::subscriber::with_default(subscriber, operation);

    let bytes = output
        .lock()
        .map_err(|error| {
            Box::new(std::io::Error::other(format!(
                "log buffer mutex poisoned: {error}"
            ))) as Box<dyn std::error::Error + Send + Sync>
        })?
        .clone();
    String::from_utf8(bytes)
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
}

pub(crate) fn require_outcome(
    actual: CommandOutcome,
    expected: CommandOutcome,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if actual == expected {
        Ok(())
    } else {
        Err(Box::new(std::io::Error::other(format!(
            "expected outcome {expected:?}, got {actual:?}"
        ))))
    }
}

pub(crate) fn require_log_contains(
    logs: &str,
    expected: &str,
    description: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if logs.contains(expected) {
        Ok(())
    } else {
        Err(Box::new(std::io::Error::other(format!(
            "warning should include {description}: {logs}"
        ))))
    }
}
