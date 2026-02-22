//! Scenario state for interactive exec behavioural tests.

use podbot::engine::ExecMode;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;

#[derive(Debug, Clone)]
pub(crate) enum ExecutionOutcome {
    Success { exit_code: i64 },
    Failure { message: String },
}

#[derive(Default, ScenarioState)]
pub(crate) struct InteractiveExecState {
    pub(crate) mode: Slot<ExecMode>,
    pub(crate) tty_enabled: Slot<bool>,
    pub(crate) command: Slot<Vec<String>>,
    pub(crate) exit_code: Slot<i64>,
    pub(crate) create_exec_should_fail: Slot<bool>,
    pub(crate) omit_exit_code: Slot<bool>,
    pub(crate) outcome: Slot<ExecutionOutcome>,
}

#[fixture]
pub(crate) fn interactive_exec_state() -> InteractiveExecState {
    let state = InteractiveExecState::default();
    state.mode.set(ExecMode::Attached);
    state.tty_enabled.set(true);
    state
        .command
        .set(vec![String::from("echo"), String::from("hello")]);
    state.exit_code.set(0);
    state.create_exec_should_fail.set(false);
    state.omit_exit_code.set(false);
    state
}
