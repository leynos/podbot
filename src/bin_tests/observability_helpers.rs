//! Observability test helpers for binary dispatch tests.
//!
//! This module keeps log capture, test fixtures, and observability-specific
//! state out of the dispatch test module so the tests remain focused on CLI
//! behaviour. It is compiled only for experimental builds where run-agent
//! observability exists.

use std::sync::{Arc, Mutex};

use clap::Parser;
use podbot::cli::Cli;
use podbot::config::AppConfig;
use rstest::fixture;

use super::run;

#[cfg(feature = "experimental")]
pub(super) struct RunObservabilityCase {
    pub(super) repo: &'static str,
    pub(super) branch: &'static str,
    pub(super) expected_log_substring: &'static str,
}

#[cfg(feature = "experimental")]
pub(super) struct CapturedRunDispatch {
    pub(super) logs: String,
}

/// Capture the logs of a `run` dispatch that must fail validation.
///
/// The default configuration has no GitHub App credentials, so every case
/// stops before any engine work; the fixture only proves the failure was
/// reported rather than silently swallowed.
#[cfg(feature = "experimental")]
#[fixture]
pub(super) fn capture_run_dispatch()
-> impl Fn(&str, &str) -> Result<CapturedRunDispatch, Box<dyn std::error::Error + Send + Sync>> {
    |repo, branch| {
        let (logs, dispatch_result) = capture_run_logs(|| dispatch_run(repo, branch))?;
        dispatch_result?;
        Ok(CapturedRunDispatch { logs })
    }
}

/// Dispatch a `run` request that must fail validation.
///
/// A dispatch that unexpectedly succeeds is reported as an error so the
/// fixture cannot pass without exercising the rejection path.
#[cfg(feature = "experimental")]
fn dispatch_run(repo: &str, branch: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::try_parse_from(["podbot", "run", "--repo", repo, "--branch", branch])?;
    match run(&cli, &AppConfig::default()) {
        Err(error) => {
            tracing::debug!(%error, "run dispatch rejected the request");
            Ok(())
        }
        Ok(_) => Err(Box::new(std::io::Error::other(
            "default configuration should fail validation",
        ))),
    }
}

#[cfg(feature = "experimental")]
#[derive(Clone)]
pub(super) struct SharedLogWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

#[cfg(feature = "experimental")]
impl std::io::Write for SharedLogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut buffer = self
            .buffer
            .lock()
            .map_err(|error| std::io::Error::other(format!("log buffer poisoned: {error}")))?;
        buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(feature = "experimental")]
pub(super) struct SharedLogBuffer {
    buffer: Arc<Mutex<Vec<u8>>>,
}

#[cfg(feature = "experimental")]
impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for SharedLogBuffer {
    type Writer = SharedLogWriter;

    fn make_writer(&'writer self) -> Self::Writer {
        SharedLogWriter {
            buffer: Arc::clone(&self.buffer),
        }
    }
}

#[cfg(feature = "experimental")]
pub(super) fn capture_run_logs<T>(
    run_test: impl FnOnce() -> T,
) -> Result<(String, T), Box<dyn std::error::Error + Send + Sync>> {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedLogBuffer {
        buffer: Arc::clone(&buffer),
    };
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .without_time()
        .with_ansi(false)
        .with_writer(writer)
        .finish();

    let output = tracing::subscriber::with_default(subscriber, run_test);

    let bytes = buffer
        .lock()
        .map_err(|error| std::io::Error::other(format!("log buffer poisoned: {error}")))?
        .clone();
    Ok((String::from_utf8(bytes)?, output))
}

#[cfg(feature = "experimental")]
pub(super) fn assert_log_contains(logs: &str, expected: &str) {
    assert!(
        logs.contains(expected),
        "expected logs to contain {expected:?}, got {logs:?}"
    );
}
