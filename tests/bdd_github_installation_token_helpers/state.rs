//! Scenario state for GitHub installation-token BDD tests.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use podbot::github::InstallationAccessToken;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;

/// Convenience alias for step outcomes.
pub type StepResult<T> = Result<T, String>;

/// Fixture token value used to verify redaction boundaries.
pub const FIXTURE_TOKEN: &str = "ghs_bdd_secret_fixture_token";

/// Type of mock API response to simulate.
#[derive(Clone, Copy, Debug, Default)]
pub enum MockTokenResponse {
    /// GitHub returns an installation access token.
    #[default]
    Success,
    /// GitHub rejects installation token acquisition.
    RejectedInstallation,
}

/// Outcome of an installation-token acquisition attempt.
#[derive(Clone, Debug)]
pub enum TokenOutcome {
    /// Token acquisition succeeded.
    Success {
        /// Token and timing metadata returned by the client.
        token: InstallationAccessToken,
    },
    /// Token acquisition failed with the given error message.
    Failed {
        /// The `Display` representation of the error.
        message: String,
    },
}

/// State shared across GitHub installation-token scenarios.
#[derive(Default, ScenarioState)]
pub struct GitHubInstallationTokenState {
    /// The mock API response type to simulate.
    pub(crate) mock_response: Slot<MockTokenResponse>,
    /// The GitHub App installation ID to use.
    pub(crate) installation_id: Slot<u64>,
    /// The configured expiry buffer.
    pub(crate) expiry_buffer: Slot<Duration>,
    /// Deterministic acquisition timestamp.
    pub(crate) acquired_at: Slot<SystemTime>,
    /// Outcome of the most recent acquisition attempt.
    pub(crate) outcome: Slot<TokenOutcome>,
    /// Shared Tokio runtime for async step execution.
    pub runtime: Slot<StepResult<Arc<tokio::runtime::Runtime>>>,
}

/// Fixture providing fresh state for each scenario.
#[rstest::fixture]
pub fn github_installation_token_state() -> GitHubInstallationTokenState {
    let state = GitHubInstallationTokenState::default();
    let runtime = tokio::runtime::Runtime::new()
        .map(Arc::new)
        .map_err(|e| format!("failed to create scenario tokio runtime: {e}"));
    state.runtime.set(runtime);
    state
}
