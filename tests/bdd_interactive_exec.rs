//! Behavioural tests for interactive container execution.

mod bdd_interactive_exec_helpers;

use bdd_interactive_exec_helpers::{InteractiveExecState, interactive_exec_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Attached execution succeeds and returns zero exit code"
)]
fn attached_execution_succeeds(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Detached execution returns non-zero exit code"
)]
fn detached_execution_returns_non_zero_exit(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Execution fails when daemon create-exec call fails"
)]
fn execution_fails_when_create_exec_fails(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Execution fails when daemon omits exit code"
)]
fn execution_fails_when_exit_code_missing(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}

#[scenario(
    path = "tests/features/interactive_exec.feature",
    name = "Attached execution with tty disabled still succeeds"
)]
fn attached_execution_with_tty_disabled_succeeds(interactive_exec_state: InteractiveExecState) {
    let _ = interactive_exec_state;
}
