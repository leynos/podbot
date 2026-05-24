//! Tests for the `podbot` binary command dispatch and CLI-owned observability.

use clap::Parser;
#[cfg(feature = "experimental")]
use podbot::api::CommandOutcome;
use podbot::cli::{Cli, Commands, HostArgs};
use podbot::config::AppConfig;
use podbot::error::{ConfigError, PodbotError};
use rstest::rstest;
#[cfg(feature = "experimental")]
use std::sync::{Arc, Mutex};

use super::{normalize_process_exit_code, run};

#[test]
fn normalize_process_exit_code_preserves_valid_range() {
    assert_eq!(normalize_process_exit_code(0), 0);
    assert_eq!(normalize_process_exit_code(42), 42);
    assert_eq!(normalize_process_exit_code(255), 255);
}

#[test]
fn normalize_process_exit_code_maps_negative_values_to_one() {
    assert_eq!(normalize_process_exit_code(-1), 1);
    assert_eq!(normalize_process_exit_code(i64::MIN), 1);
}

#[test]
fn normalize_process_exit_code_clamps_oversized_values() {
    assert_eq!(normalize_process_exit_code(256), 255);
    assert_eq!(normalize_process_exit_code(i64::MAX), 255);
}

#[test]
fn run_rejects_host_subcommand_until_stdout_is_safe() {
    let cli = Cli {
        command: Commands::Host(HostArgs {
            agent: None,
            mode: None,
        }),
        config: None,
        engine_socket: None,
        image: None,
    };

    let error = run(&cli, &AppConfig::default()).expect_err("host command should be disabled");

    assert!(
        error
            .to_string()
            .contains("host subcommand is temporarily disabled"),
        "unexpected error: {error}",
    );
}

#[test]
#[cfg(feature = "experimental")]
fn run_dispatches_cli_request_to_run_agent_api() {
    let cli = Cli::try_parse_from(["podbot", "run", "--repo", "owner/name", "--branch", "main"])
        .expect("run command should parse");

    let outcome = run(&cli, &AppConfig::default()).expect("run dispatch should succeed");

    assert_eq!(outcome, CommandOutcome::Success);
}

#[test]
#[cfg(feature = "experimental")]
fn run_observability_logs_distinct_cli_request_values() {
    let logs = capture_run_logs(|| {
        let cli = Cli::try_parse_from([
            "podbot",
            "run",
            "--repo",
            "team/service",
            "--branch",
            "feature/observability",
        ])
        .expect("run command should parse");

        run(&cli, &AppConfig::default()).expect("run dispatch should succeed");
    })
    .expect("logs should be captured");

    assert_log_contains(&logs, "run_agent completed successfully");
    assert_log_contains(&logs, "team/service");
    assert_log_contains(&logs, "feature/observability");
}

#[test]
#[cfg(feature = "experimental")]
fn run_observability_logs_distinct_cli_request_values_on_failure() {
    let logs = capture_run_logs(|| {
        let cli = Cli::try_parse_from([
            "podbot",
            "run",
            "--repo",
            "owner/failing-service",
            "--branch",
            "release/failed-validation",
        ])
        .expect("run command should parse");
        let config = AppConfig {
            github: podbot::config::GitHubConfig {
                app_id: Some(1),
                installation_id: None,
                private_key_path: None,
            },
            ..AppConfig::default()
        };

        run(&cli, &config).expect_err("incomplete GitHub config should fail");
    })
    .expect("logs should be captured");

    assert_log_contains(&logs, "run_agent validation failed for run request");
    assert_log_contains(&logs, "owner/failing-service");
    assert_log_contains(&logs, "release/failed-validation");
}

#[test]
#[cfg(not(feature = "experimental"))]
fn run_dispatches_cli_request_to_experimental_gate() {
    let cli = Cli::try_parse_from(["podbot", "run", "--repo", "owner/name", "--branch", "main"])
        .expect("run command should parse");

    let error = run(&cli, &AppConfig::default()).expect_err("run should require experimental");

    assert_experimental_only_error(error, "run");
}

#[rstest]
#[case("", "main", "run.repository")]
#[case("   ", "main", "run.repository")]
#[case("owner/name", "", "run.branch")]
#[case("owner/name", "   ", "run.branch")]
fn run_rejects_invalid_run_arguments(
    #[case] repo: &str,
    #[case] branch: &str,
    #[case] expected_field: &str,
) {
    let cli = Cli::try_parse_from(["podbot", "run", "--repo", repo, "--branch", branch])
        .expect("run command should parse");

    let error =
        run(&cli, &AppConfig::default()).expect_err("invalid run argument should be rejected");

    assert_invalid_run_argument(error, expected_field);
}

#[test]
#[cfg(not(feature = "experimental"))]
fn non_experimental_stubs_report_command_names() {
    let config = AppConfig::default();
    let request =
        podbot::api::RunRequest::new("owner/name", "main").expect("run request should be valid");

    let cases = [
        ("run", super::run_agent_api(&config, &request)),
        ("token-daemon", super::run_token_daemon_api("test-ctr")),
        ("ps", super::list_containers_api()),
        ("stop", super::stop_container_api("test-ctr")),
    ];

    for (command, result) in cases {
        let error = result.expect_err("stub should require experimental");
        assert_experimental_only_error(error, command);
    }
}

#[cfg(not(feature = "experimental"))]
fn assert_experimental_only_error(error: PodbotError, command: &str) {
    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::InvalidValue { field, reason })
            if field == "command"
                && reason == format!("the {command} command requires feature = \"experimental\"")
    ));
}

fn assert_invalid_run_argument(error: PodbotError, expected_field: &str) {
    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::InvalidValue { field, reason })
            if field == expected_field
                && reason == format!("{expected_field} must not be empty")
    ));
}

#[cfg(feature = "experimental")]
#[derive(Clone)]
struct SharedLogWriter {
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
struct SharedLogBuffer {
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
fn capture_run_logs(
    run_test: impl FnOnce(),
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedLogBuffer {
        buffer: Arc::clone(&buffer),
    };
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
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
fn assert_log_contains(logs: &str, expected: &str) {
    assert!(
        logs.contains(expected),
        "expected logs to contain {expected:?}, got {logs:?}"
    );
}
