//! Then-step assertions for interactive execution scenarios.

use rstest_bdd_macros::then;

use super::state::{ExecutionOutcome, InteractiveExecState};
use super::steps::StepResult;

fn get_recorded_outcome(state: &InteractiveExecState) -> StepResult<ExecutionOutcome> {
    state
        .outcome
        .get()
        .ok_or_else(|| String::from("execution outcome should be recorded"))
}

#[then("execution succeeds")]
fn execution_succeeds(interactive_exec_state: &InteractiveExecState) -> StepResult<()> {
    let outcome = get_recorded_outcome(interactive_exec_state)?;

    match outcome {
        ExecutionOutcome::Success { .. } => Ok(()),
        ExecutionOutcome::Failure { message } => {
            Err(format!("expected success, got failure: {message}"))
        }
    }
}

#[then("reported exit code is {code}")]
fn reported_exit_code_is(
    interactive_exec_state: &InteractiveExecState,
    code: i64,
) -> StepResult<()> {
    let outcome = get_recorded_outcome(interactive_exec_state)?;

    match outcome {
        ExecutionOutcome::Success { exit_code } if exit_code == code => Ok(()),
        ExecutionOutcome::Success { exit_code } => {
            Err(format!("expected exit code {code}, got {exit_code}"))
        }
        ExecutionOutcome::Failure { message } => Err(format!(
            "expected success with exit code {code}, got failure: {message}"
        )),
    }
}

// Note: This assertion depends on the error message format defined in
// src/error.rs ContainerError::ExecFailed. If that message changes, this test
// must be updated accordingly.
#[then("execution fails with an exec error")]
fn execution_fails_with_exec_error(
    interactive_exec_state: &InteractiveExecState,
) -> StepResult<()> {
    let outcome = get_recorded_outcome(interactive_exec_state)?;

    match outcome {
        ExecutionOutcome::Failure { message }
            if message.contains("failed to execute command in container") =>
        {
            Ok(())
        }
        ExecutionOutcome::Failure { message } => {
            Err(format!("expected exec failure message, got: {message}"))
        }
        ExecutionOutcome::Success { exit_code } => Err(format!(
            "expected failure, got success with exit code {exit_code}"
        )),
    }
}

// Note: This assertion depends on the error message format in
// src/engine/connection/exec/attached.rs wait_for_exit_code_async. If that
// message changes, this test must be updated accordingly.
#[then("execution fails due to missing exit code")]
fn execution_fails_due_to_missing_exit_code(
    interactive_exec_state: &InteractiveExecState,
) -> StepResult<()> {
    let outcome = get_recorded_outcome(interactive_exec_state)?;

    match outcome {
        ExecutionOutcome::Failure { message } if message.contains("without an exit code") => Ok(()),
        ExecutionOutcome::Failure { message } => Err(format!(
            "expected missing-exit-code failure message, got: {message}"
        )),
        ExecutionOutcome::Success { exit_code } => Err(format!(
            "expected failure, got success with exit code {exit_code}"
        )),
    }
}
