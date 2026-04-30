//! Scenario state for library boundary behavioural tests.
//!
//! Defines the `LibraryBoundaryState` struct that carries intermediate results
//! between BDD steps — configuration loading outcomes, exec results, mock
//! failure flags, and stub orchestration outcomes.

use std::sync::Arc;

use podbot::api::{CommandOutcome, RunRequest};
use podbot::config::AppConfig;
use podbot::error::PodbotError;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;

/// High-level outcome from a library call.
#[derive(Debug, Clone)]
pub(crate) enum LibraryResult {
    /// Library call returned a `CommandOutcome`.
    Ok(CommandOutcome),
    /// Library call returned an error.
    /// Wrapped in Arc to allow cloning for BDD state management.
    Err(Arc<PodbotError>),
}

/// High-level config loading result.
#[derive(Debug, Clone)]
pub(crate) enum ConfigResult {
    /// Configuration loaded successfully.
    Ok(Box<AppConfig>),
    /// Configuration loading failed.
    /// Wrapped in Arc to allow cloning for BDD state management.
    Err(Arc<PodbotError>),
}

/// Collected outcomes from stub orchestration functions.
#[derive(Debug, Clone)]
pub(crate) struct StubOutcomes {
    /// Results from `run_agent`, `list_containers`, `stop_container`,
    /// and `run_token_daemon`.
    pub(crate) results: Vec<LibraryResult>,
}

#[derive(Default, ScenarioState)]
pub(crate) struct LibraryBoundaryState {
    pub(crate) engine_socket_override: Slot<String>,
    pub(crate) config_result: Slot<ConfigResult>,
    pub(crate) exec_result: Slot<LibraryResult>,
    pub(crate) run_request: Slot<RunRequest>,
    pub(crate) create_exec_should_fail: Slot<bool>,
    pub(crate) stub_outcomes: Slot<StubOutcomes>,
}

#[fixture]
pub(crate) fn library_boundary_state() -> LibraryBoundaryState {
    let state = LibraryBoundaryState::default();
    state.create_exec_should_fail.set(false);
    state
}
