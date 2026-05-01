//! Shared behavioural-test state for repository-cloning scenarios.

use std::sync::{Arc, Mutex};

use podbot::engine::RepositoryCloneResult;
use podbot::error::PodbotError;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;

/// Step result type for repository-cloning BDD tests.
pub type StepResult<T> = Result<T, String>;

/// Shared scenario state for repository-cloning behavioural tests.
#[derive(Default, ScenarioState)]
pub struct RepositoryCloningState {
    /// Raw repository input supplied by the operator.
    pub(crate) repository_input: Slot<String>,
    /// Raw branch input supplied by the operator.
    pub(crate) branch_input: Slot<String>,
    /// Workspace base directory inside the container.
    pub(crate) workspace_base_dir: Slot<String>,
    /// `GIT_ASKPASS` helper path inside the container.
    pub(crate) askpass_path: Slot<String>,
    /// Exit code for the clone command.
    pub(crate) clone_exit_code: Slot<i64>,
    /// Exit code for the branch verification command.
    pub(crate) verification_exit_code: Slot<i64>,
    /// Captured exec commands and environments.
    pub(crate) observed_execs: Slot<Arc<Mutex<Vec<ObservedExec>>>>,
    /// Outcome of the most recent clone attempt.
    pub(crate) outcome: Slot<Result<RepositoryCloneResult, PodbotError>>,
}

/// Captured container exec request data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedExec {
    /// Command argv entries.
    pub command: Vec<String>,
    /// Environment entries in `KEY=value` form.
    pub env: Vec<String>,
}

/// Fixture providing fresh state for each repository-cloning scenario.
#[fixture]
pub fn repository_cloning_state() -> RepositoryCloningState {
    let state = RepositoryCloningState::default();
    state.clone_exit_code.set(0);
    state.verification_exit_code.set(0);
    state.observed_execs.set(Arc::new(Mutex::new(Vec::new())));
    state
}
