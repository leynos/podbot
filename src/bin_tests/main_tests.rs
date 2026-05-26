//! Tests for the `podbot` binary command dispatch and CLI-owned observability.

#[cfg(feature = "experimental")]
use chrono::TimeZone;
use clap::{CommandFactory, Parser};
#[cfg(feature = "experimental")]
use podbot::api::CommandOutcome;
use podbot::cli::{Cli, Commands, HostArgs};
use podbot::config::AppConfig;
use podbot::error::{ConfigError, PodbotError};
use rstest::rstest;

#[cfg(feature = "experimental")]
use super::run_agent_api_with_observability;
use super::{normalize_process_exit_code, run};

#[cfg(feature = "experimental")]
mod observability_helpers;
#[cfg(feature = "experimental")]
use observability_helpers::{
    CapturedRunDispatch, RunObservabilityCase, assert_log_contains, capture_run_dispatch,
    capture_run_logs,
};

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
fn run_help_output_matches_snapshot() {
    let mut command = Cli::command();
    let run_help = command
        .find_subcommand_mut("run")
        .expect("run subcommand should be registered")
        .render_long_help()
        .to_string();

    insta::assert_snapshot!(run_help, @r"
Run an AI agent in a sandboxed container

Usage: run [OPTIONS] --repo <REPO> --branch <BRANCH>

Options:
      --repo <REPO>
          Repository to clone in owner/name format

      --branch <BRANCH>
          Branch to check out

      --agent <AGENT>
          Agent type to run

          Possible values:
          - claude: Claude Code agent
          - codex:  `OpenAI` Codex agent
          - custom: Custom operator-supplied agent launcher

      --agent-mode <MODE>
          Agent execution mode

          Possible values:
          - podbot:           Run the agent in podbot-managed mode
          - codex_app_server: Run the agent as a Codex App Server
          - acp:              Run the agent as an ACP server

  -h, --help
          Print help (see a summary with '-h')
");
}
#[test]
fn run_validation_error_matches_snapshot() {
    let cli = Cli::try_parse_from(["podbot", "run", "--repo", "   ", "--branch", "main"])
        .expect("run command should parse");
    let error = run(&cli, &AppConfig::default()).expect_err("invalid run argument should fail");

    insta::assert_snapshot!(
        error.to_string(),
        @"invalid configuration value for 'run.repository': run.repository must not be empty"
    );
}

#[rstest]
#[cfg(feature = "experimental")]
#[case::success(RunObservabilityCase {
    repo: "team/service",
    branch: "feature/observability",
    expect_success: true,
    expected_log_substring: "run_agent completed successfully",
})]
#[case::failure(RunObservabilityCase {
    repo: "owner/failing-service",
    branch: "release/failed-validation",
    expect_success: false,
    expected_log_substring: "run_agent validation failed for run request",
})]
fn run_observability_logs_distinct_cli_request_values(
    capture_run_dispatch: impl Fn(
        &str,
        &str,
        bool,
    ) -> Result<
        CapturedRunDispatch,
        Box<dyn std::error::Error + Send + Sync>,
    >,
    #[case] test_case: RunObservabilityCase,
) {
    let captured = capture_run_dispatch(test_case.repo, test_case.branch, test_case.expect_success)
        .expect("run logs should be captured");

    assert_eq!(captured.succeeded, test_case.expect_success);
    assert_log_contains(&captured.logs, test_case.expected_log_substring);
    assert_log_contains(&captured.logs, test_case.repo);
    assert_log_contains(&captured.logs, test_case.branch);
}

#[test]
#[cfg(feature = "experimental")]
fn run_observability_logs_match_snapshot() {
    let logs = capture_run_logs(|| {
        let cli = Cli::try_parse_from([
            "podbot",
            "run",
            "--repo",
            "team/snapshot-service",
            "--branch",
            "feature/snapshot",
        ])
        .expect("run command should parse");

        run(&cli, &AppConfig::default()).expect("run dispatch should succeed");
    })
    .expect("run logs should be captured");

    insta::assert_snapshot!(logs, @r#"
DEBUG podbot: validating run request before agent orchestration operation="run_agent" repository="team/snapshot-service" branch="feature/snapshot"
DEBUG podbot::api: GitHub configuration validation skipped for run request operation="run_agent" repository="team/snapshot-service" branch="feature/snapshot"
DEBUG podbot::api: GitHub credential validation skipped for run request operation="run_agent" repository="team/snapshot-service" branch="feature/snapshot"
DEBUG podbot: run_agent completed successfully operation="run_agent" repository="team/snapshot-service" branch="feature/snapshot" outcome="success"
"#);
}

#[test]
#[cfg(feature = "experimental")]
fn run_observability_uses_injected_clock() {
    let mut clock = mockable::MockClock::new();
    let started_at = chrono::Utc
        .with_ymd_and_hms(2026, 5, 25, 12, 0, 0)
        .single()
        .expect("start time should be valid");
    let finished_at = chrono::Utc
        .with_ymd_and_hms(2026, 5, 25, 12, 0, 2)
        .single()
        .expect("finish time should be valid");
    clock.expect_utc().times(1).return_once(move || started_at);
    clock.expect_utc().times(1).return_once(move || finished_at);

    let request =
        podbot::api::RunRequest::new("owner/name", "main").expect("run request should be valid");
    let outcome = run_agent_api_with_observability(&AppConfig::default(), &request, &clock)
        .expect("run dispatch should succeed");

    assert_eq!(outcome, CommandOutcome::Success);
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
