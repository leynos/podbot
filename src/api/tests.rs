//! Unit tests for the orchestration API module.

use rstest::rstest;

use super::{CommandOutcome, list_containers, run_agent, run_token_daemon, stop_container};
use crate::config::AppConfig;

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
