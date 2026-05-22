//! Scenario state for orchestration behavioural tests.
//!
//! The state model holds the slots shared across orchestration scenarios, so
//! the step definitions can coordinate command inputs, execution mode, and
//! results without repeating fixture setup.
//! It is the bridge between Given steps that configure requests, When steps
//! that invoke orchestration APIs, and Then assertions that inspect the stored
//! outcomes.

use podbot::api::{CommandOutcome, ExecMode};
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
