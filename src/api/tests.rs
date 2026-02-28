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
fn run_agent_stub_returns_success() {
    let config = AppConfig::default();
    let outcome = run_agent(&config).expect("run_agent stub should succeed");
    assert_eq!(outcome, CommandOutcome::Success);
}

#[rstest]
fn list_containers_stub_returns_success() {
    let outcome = list_containers().expect("list_containers stub should succeed");
    assert_eq!(outcome, CommandOutcome::Success);
}

#[rstest]
fn stop_container_stub_returns_success() {
    let outcome = stop_container("test-container").expect("stop_container stub should succeed");
    assert_eq!(outcome, CommandOutcome::Success);
}

#[rstest]
fn run_token_daemon_stub_returns_success() {
    let outcome =
        run_token_daemon("test-container-id").expect("run_token_daemon stub should succeed");
    assert_eq!(outcome, CommandOutcome::Success);
}
