//! Scenario state for installation-token Behaviour-Driven Development tests.

use std::sync::Arc;
use std::time::Duration;

use camino::Utf8PathBuf;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;
use tempfile::TempDir;

/// Convenience alias for step outcomes.
pub type StepResult<T> = Result<T, String>;

/// Type of mock token response to simulate.
#[derive(Clone, Copy, Debug, Default)]
pub enum MockInstallationTokenResponse {
    /// GitHub returns a valid token whose expiry is outside the buffer.
    #[default]
    Success,
    /// GitHub returns a token that expires within the requested buffer.
    NearExpiry,
    /// GitHub rejects the token request.
    ApiRejected,
    /// GitHub omits `expires_at` from the response.
    MissingExpiry,
}

/// Outcome of an installation-token acquisition attempt.
#[derive(Clone, Debug)]
pub enum InstallationTokenOutcome {
    /// Token acquisition succeeded.
    Success {
        /// The returned token string.
        token: String,
        /// The returned expiry timestamp in RFC 3339 format.
        expires_at: String,
    },
    /// Token acquisition failed with a displayable message.
    Failed {
        /// The error message.
        message: String,
    },
}

/// State shared across installation-token scenarios.
#[derive(Default, ScenarioState)]
pub struct GitHubInstallationTokenState {
    /// Temporary directory backing the test key file.
    pub(crate) temp_dir: Slot<Arc<TempDir>>,
    /// Path to the configured private key.
    pub(crate) key_path: Slot<Utf8PathBuf>,
    /// The GitHub App ID under test.
    pub(crate) app_id: Slot<u64>,
    /// The GitHub installation ID under test.
    pub(crate) installation_id: Slot<u64>,
    /// The requested expiry buffer.
    pub(crate) buffer: Slot<Duration>,
    /// Mock response type to simulate.
    pub(crate) mock_response: Slot<MockInstallationTokenResponse>,
    /// Outcome of the acquisition attempt.
    pub(crate) outcome: Slot<InstallationTokenOutcome>,
}

/// Fixture providing fresh state for each scenario.
#[rstest::fixture]
pub fn github_installation_token_state() -> GitHubInstallationTokenState {
    GitHubInstallationTokenState::default()
}
