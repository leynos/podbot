//! Shared behavioural-test state for Git identity scenarios.

use podbot::engine::GitIdentityResult;
use podbot::error::PodbotError;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;

/// Step result type for Git identity BDD tests.
pub type StepResult<T> = Result<T, String>;

/// Shared scenario state for Git identity behavioural tests.
#[derive(Default, ScenarioState)]
pub struct GitIdentityState {
    /// Host Git user.name value, if configured.
    pub(crate) host_name: Slot<Option<String>>,

    /// Host Git user.email value, if configured.
    pub(crate) host_email: Slot<Option<String>>,

    /// Target container ID for configuration.
    pub(crate) container_id: Slot<String>,

    /// Whether the container engine should fail exec calls.
    pub(crate) should_fail_exec: Slot<bool>,

    /// Outcome of the most recent configuration attempt.
    pub(crate) outcome: Slot<Result<GitIdentityResult, PodbotError>>,
}

/// Fixture providing fresh state for each Git identity scenario.
#[fixture]
pub fn git_identity_state() -> GitIdentityState {
    let state = GitIdentityState::default();
    state.container_id.set(String::from("sandbox-default"));
    state.should_fail_exec.set(false);
    state
}
