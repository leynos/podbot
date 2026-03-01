//! Assertion helpers for orchestration behavioural tests.

use podbot::api::CommandOutcome;
use rstest_bdd_macros::then;

use super::StepResult;
use super::state::{OrchestrationResult, OrchestrationState};

#[then("the outcome is success")]
fn outcome_is_success(orchestration_state: &OrchestrationState) -> StepResult<()> {
    let result = orchestration_state
        .result
        .get()
        .ok_or_else(|| String::from("result should be set"))?;

    match result {
        OrchestrationResult::Ok(CommandOutcome::Success) => Ok(()),
        OrchestrationResult::Ok(CommandOutcome::CommandExit { code }) => Err(format!(
            "expected Success, got CommandExit {{ code: {code} }}"
        )),
        OrchestrationResult::Err(msg) => Err(format!("expected Success, got error: {msg}")),
    }
}

#[then("the outcome is command exit with code {expected_code}")]
fn outcome_is_command_exit(
    orchestration_state: &OrchestrationState,
    expected_code: i64,
) -> StepResult<()> {
    let result = orchestration_state
        .result
        .get()
        .ok_or_else(|| String::from("result should be set"))?;

    match result {
        OrchestrationResult::Ok(CommandOutcome::CommandExit { code }) if code == expected_code => {
            Ok(())
        }
        OrchestrationResult::Ok(CommandOutcome::CommandExit { code }) => {
            Err(format!("expected exit code {expected_code}, got {code}"))
        }
        OrchestrationResult::Ok(CommandOutcome::Success) => Err(format!(
            "expected CommandExit {{ code: {expected_code} }}, got Success"
        )),
        OrchestrationResult::Err(msg) => Err(format!(
            "expected CommandExit {{ code: {expected_code} }}, got error: {msg}"
        )),
    }
}
