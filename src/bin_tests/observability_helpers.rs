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
    pub(super) expect_success: bool,
    pub(super) expected_log_substring: &'static str,
}

#[cfg(feature = "experimental")]
pub(super) struct CapturedRunDispatch {
    pub(super) logs: String,
    pub(super) succeeded: bool,
}

#[cfg(feature = "experimental")]
#[fixture]
pub(super) fn capture_run_dispatch()
-> impl Fn(&str, &str, bool) -> Result<CapturedRunDispatch, Box<dyn std::error::Error + Send + Sync>>
{
    |repo, branch, expect_success| {
        let mut succeeded = false;
        let logs = capture_run_logs(|| {
            let cli = Cli::try_parse_from(["podbot", "run", "--repo", repo, "--branch", branch])
                .expect("run command should parse");
            let config = run_observability_config(expect_success);
            let result = run(&cli, &config);

            if expect_success {
                result.expect("run dispatch should succeed");
                succeeded = true;
            } else {
                result.expect_err("incomplete GitHub config should fail");
            }
        })?;

        Ok(CapturedRunDispatch { logs, succeeded })
    }
}

#[cfg(feature = "experimental")]
pub(super) fn run_observability_config(expect_success: bool) -> AppConfig {
    if expect_success {
        AppConfig::default()
    } else {
        AppConfig {
            github: podbot::config::GitHubConfig {
                app_id: Some(1),
                installation_id: None,
                private_key_path: None,
            },
            ..AppConfig::default()
        }
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
pub(super) fn capture_run_logs(
    run_test: impl FnOnce(),
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedLogBuffer {
        buffer: Arc::clone(&buffer),
    };
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .without_time()
        .with_writer(writer)
        .finish();

    tracing::subscriber::with_default(subscriber, run_test);

    let bytes = buffer
        .lock()
        .map_err(|error| std::io::Error::other(format!("log buffer poisoned: {error}")))?
        .clone();
    Ok(String::from_utf8(bytes)?)
}

#[cfg(feature = "experimental")]
pub(super) fn assert_log_contains(logs: &str, expected: &str) {
    assert!(
        logs.contains(expected),
        "expected logs to contain {expected:?}, got {logs:?}"
    );
}
