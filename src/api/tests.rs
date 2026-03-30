//! Unit tests for the orchestration API module.

use rstest::rstest;

use super::{
    CommandOutcome, ExecMode, ExecRequest, exec, list_containers, run_agent, run_token_daemon,
    stop_container,
};
use crate::config::AppConfig;
use crate::error::{ConfigError, PodbotError};

#[rstest]
fn command_outcome_success_equals_itself() {
    assert_eq!(CommandOutcome::Success, CommandOutcome::Success);
}

#[rstest]
fn command_outcome_exit_preserves_code() {
    let outcome = CommandOutcome::CommandExit { code: 42 };
    assert_eq!(outcome, CommandOutcome::CommandExit { code: 42 });
}

#[rstest]
fn command_outcome_success_differs_from_exit_zero() {
    assert_ne!(
        CommandOutcome::Success,
        CommandOutcome::CommandExit { code: 0 }
    );
}

#[rstest]
fn command_outcome_is_copy() {
    let outcome = CommandOutcome::CommandExit { code: 7 };
    let copied = outcome;
    assert_eq!(outcome, copied);
}

#[rstest]
fn exec_request_defaults_to_attached_without_tty() {
    let request =
        ExecRequest::new("sandbox", vec![String::from("echo")]).expect("request should be valid");

    assert_eq!(request.mode, ExecMode::Attached);
    assert!(!request.tty);
}

#[rstest]
fn exec_request_rejects_blank_container() {
    let error = ExecRequest::new("   ", vec![String::from("echo")])
        .expect_err("blank container should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field }) if field == "container"
    ));
}

#[rstest]
fn exec_request_rejects_blank_executable() {
    let error = ExecRequest::new("sandbox", vec![String::from("  ")])
        .expect_err("blank executable should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field }) if field == "command[0]"
    ));
}

#[rstest]
fn exec_rejects_invalid_request_before_engine_connection() {
    let config = AppConfig::default();
    let request = ExecRequest {
        container: String::from("sandbox"),
        command: Vec::new(),
        mode: ExecMode::Attached,
        tty: false,
    };

    let error = exec(&config, &request).expect_err("invalid request should fail fast");
    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field }) if field == "command"
    ));
}

#[rstest]
#[case::run_agent("run_agent")]
#[case::list_containers("list_containers")]
#[case::stop_container("stop_container")]
#[case::run_token_daemon("run_token_daemon")]
fn stub_returns_success(#[case] stub: &str) {
    let config = AppConfig::default();
    let outcome = match stub {
        "run_agent" => run_agent(&config),
        "list_containers" => list_containers(),
        "stop_container" => stop_container("test-container"),
        "run_token_daemon" => run_token_daemon("test-container-id"),
        other => panic!("unknown stub: {other}"),
    }
    .expect("stub should return Ok");
    assert_eq!(outcome, CommandOutcome::Success);
}
