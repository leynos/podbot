//! Behavioural tests for interactive container execution.

#![cfg(feature = "internal")]

mod bdd_interactive_exec_helpers;
/// Scoped stdin selection used only by this suite and orchestration/exec peers.
#[path = "test_support/stdin_forwarding.rs"]
mod stdin_forwarding;
mod test_utils;

use bdd_interactive_exec_helpers::{InteractiveExecState, interactive_exec_state};
use rstest_bdd_macros::scenario;
use serial_test::serial;

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Attached execution succeeds and returns zero exit code"
)]
#[serial]
fn attached_execution_succeeds(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Detached execution returns non-zero exit code"
)]
#[serial]
fn detached_execution_returns_non_zero_exit(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Execution fails when daemon create-exec call fails"
)]
#[serial]
fn execution_fails_when_create_exec_fails(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Execution fails when daemon omits exit code"
)]
#[serial]
fn execution_fails_when_exit_code_missing(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Attached execution with tty disabled still succeeds"
)]
#[serial]
fn attached_execution_with_tty_disabled_succeeds(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Protocol execution succeeds with tty disabled"
)]
#[serial]
fn protocol_execution_succeeds_with_tty_disabled(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Protocol execution returns non-zero exit code"
)]
#[serial]
fn protocol_execution_returns_non_zero_exit(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[rstest::rstest]
#[case::attached_disabled(podbot::engine::ExecMode::Attached, "1", false)]
#[case::attached_enabled(podbot::engine::ExecMode::Attached, "0", true)]
#[case::protocol_disabled(podbot::engine::ExecMode::Protocol, "1", false)]
#[case::protocol_enabled(podbot::engine::ExecMode::Protocol, "0", true)]
fn production_stdin_forwarding(
    #[case] mode: podbot::engine::ExecMode,
    #[case] setting: &str,
    #[case] should_forward: bool,
) {
    use bdd_interactive_exec_helpers::stdin_forwarding::{
        CaseExecution, INPUT_MARKER, exercise_case,
    };

    let result = exercise_case(mode, setting).expect("production stdin case should complete");
    match result {
        CaseExecution::Direct { bytes, exit_code } => {
            let expected = if should_forward { INPUT_MARKER } else { &[] };
            assert_eq!(
                bytes, expected,
                "capture actual exec-input bytes for {mode:?}/{setting}"
            );
            assert_eq!(
                exit_code, 0,
                "the selected stdin path must shut down successfully"
            );
        }
        CaseExecution::Subprocess { output, case_name } => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                output.status.success(),
                "child {case_name} failed: {stdout}\n{stderr}"
            );
            assert!(
                stdout.contains(&case_name),
                "the child must execute its exact case: {stdout}"
            );
            assert!(
                stdout.contains("1 passed; 0 failed; 0 ignored"),
                "zero-test or ignored children cannot pass: {stdout}"
            );
        }
    }
}
