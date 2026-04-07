//! Shared behavioural-test state for Git identity configuration scenarios.

use podbot::engine::GitIdentityResult;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;

/// Step result type for Git identity BDD tests.
pub type StepResult<T> = Result<T, String>;

/// Shared scenario state for Git identity behavioural tests.
#[derive(Default, ScenarioState)]
pub struct GitIdentityState {
    /// Host Git user.name value (None = not configured).
    pub(crate) host_name: Slot<Option<String>>,

    /// Host Git user.email value (None = not configured).
    pub(crate) host_email: Slot<Option<String>>,

    /// Whether the mocked container exec should fail.
    pub(crate) should_fail_exec: Slot<bool>,

    /// Outcome of the most recent Git identity configuration attempt.
    pub(crate) outcome: Slot<Option<GitIdentityResult>>,

    /// Captured exec commands forwarded to the container.
    pub(crate) captured_commands: Slot<Vec<Vec<String>>>,
}

/// Fixture providing fresh state for each Git identity scenario.
#[fixture]
pub fn git_identity_state() -> GitIdentityState {
    let state = GitIdentityState::default();
    state.host_name.set(None);
    state.host_email.set(None);
    state.should_fail_exec.set(false);
    state.captured_commands.set(Vec::new());
    state
}
