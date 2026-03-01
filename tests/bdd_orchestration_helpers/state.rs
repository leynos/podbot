//! Scenario state for orchestration behavioural tests.

use podbot::api::CommandOutcome;
use podbot::engine::ExecMode;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;

/// High-level outcome from an orchestration call.
#[derive(Debug, Clone)]
pub(crate) enum OrchestrationResult {
    /// Orchestration returned a `CommandOutcome`.
    Ok(CommandOutcome),
    /// Orchestration returned an error.
    Err(String),
}

#[derive(Default, ScenarioState)]
pub(crate) struct OrchestrationState {
    pub(crate) mode: Slot<ExecMode>,
    pub(crate) tty: Slot<bool>,
    pub(crate) command: Slot<Vec<String>>,
    pub(crate) exit_code: Slot<i64>,
    pub(crate) create_exec_should_fail: Slot<bool>,
    pub(crate) result: Slot<OrchestrationResult>,
}

#[fixture]
pub(crate) fn orchestration_state() -> OrchestrationState {
    let state = OrchestrationState::default();
    state.mode.set(ExecMode::Attached);
    state.tty.set(true);
    state
        .command
        .set(vec![String::from("echo"), String::from("hello")]);
    state.exit_code.set(0);
    state.create_exec_should_fail.set(false);
    state
}
