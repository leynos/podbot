//! Behavioural tests for interactive container execution.

mod bdd_interactive_exec_helpers;
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
