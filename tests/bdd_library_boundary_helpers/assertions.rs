//! Assertion helpers for library boundary behavioural tests.

use podbot::api::CommandOutcome;
use rstest_bdd_macros::then;

use super::StepResult;
use super::state::{ConfigResult, LibraryBoundaryState, LibraryResult};

#[then("a valid AppConfig is returned")]
fn config_is_valid(library_boundary_state: &LibraryBoundaryState) -> StepResult<()> {
    let result = library_boundary_state
        .config_result
        .get()
        .ok_or_else(|| String::from("config_result should be set"))?;

    match result {
        ConfigResult::Ok(_) => Ok(()),
        ConfigResult::Err(msg) => Err(format!("expected valid AppConfig, got error: {msg}")),
    }
}

#[then("the engine socket matches the override value")]
fn engine_socket_matches(library_boundary_state: &LibraryBoundaryState) -> StepResult<()> {
    let result = library_boundary_state
        .config_result
        .get()
        .ok_or_else(|| String::from("config_result should be set"))?;

    let expected = library_boundary_state
        .engine_socket_override
        .get()
        .ok_or_else(|| String::from("engine_socket_override should be set"))?;

    match result {
        ConfigResult::Ok(config) => {
            let actual = config
                .engine_socket
                .as_deref()
                .ok_or_else(|| String::from("engine_socket should be set in config"))?;
            if actual == expected {
                Ok(())
            } else {
                Err(format!(
                    "expected engine_socket '{expected}', got '{actual}'"
                ))
            }
        }
        ConfigResult::Err(msg) => Err(format!("expected valid config, got error: {msg}")),
    }
}

#[then("the outcome is success")]
fn outcome_is_success(library_boundary_state: &LibraryBoundaryState) -> StepResult<()> {
    let result = library_boundary_state
        .exec_result
        .get()
        .ok_or_else(|| String::from("exec_result should be set"))?;

    match result {
        LibraryResult::Ok(CommandOutcome::Success) => Ok(()),
        LibraryResult::Ok(CommandOutcome::CommandExit { code }) => Err(format!(
            "expected Success, got CommandExit {{ code: {code} }}"
        )),
        LibraryResult::Err(msg) => Err(format!("expected Success, got error: {msg}")),
    }
}

#[then("the error is a ContainerError variant")]
fn error_is_container_error(library_boundary_state: &LibraryBoundaryState) -> StepResult<()> {
    let result = library_boundary_state
        .exec_result
        .get()
        .ok_or_else(|| String::from("exec_result should be set"))?;

    match result {
        LibraryResult::Err(msg) => {
            // ContainerError::ExecFailed messages start with "failed to execute
            // command in container".
            if msg.contains("failed to execute command in container") {
                Ok(())
            } else {
                Err(format!("expected ContainerError message, got: {msg}"))
            }
        }
        LibraryResult::Ok(outcome) => Err(format!("expected ContainerError, got Ok({outcome:?})")),
    }
}

#[then("all outcomes are success")]
fn all_stubs_succeed(library_boundary_state: &LibraryBoundaryState) -> StepResult<()> {
    let outcomes = library_boundary_state
        .stub_outcomes
        .get()
        .ok_or_else(|| String::from("stub_outcomes should be set"))?;

    for (i, result) in outcomes.results.iter().enumerate() {
        match result {
            LibraryResult::Ok(CommandOutcome::Success) => {}
            LibraryResult::Ok(CommandOutcome::CommandExit { code }) => {
                return Err(format!("stub {i} returned CommandExit {{ code: {code} }}"));
            }
            LibraryResult::Err(msg) => {
                return Err(format!("stub {i} returned error: {msg}"));
            }
        }
    }
    Ok(())
}
