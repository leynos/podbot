//! Scenario state for GitHub App credential validation BDD tests.

use std::sync::Arc;

use camino::Utf8PathBuf;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;
use tempfile::TempDir;

/// Convenience alias for step outcomes.
pub type StepResult<T> = Result<T, String>;

/// Type of mock API response to simulate.
#[derive(Clone, Debug, Default)]
pub enum MockApiResponse {
    /// The API accepts the credentials.
    #[default]
    Success,
    /// The API rejects the credentials (401 Unauthorized).
    InvalidCredentials,
    /// The API returns a server error (500).
    ServerError,
}

/// Outcome of a credential validation attempt.
#[derive(Clone, Debug)]
pub enum ValidationOutcome {
    /// Validation succeeded.
    Success,
    /// Validation failed with the given error message.
    Failed {
        /// The `Display` representation of the error.
        message: String,
    },
}

/// State shared across GitHub credential validation scenarios.
#[derive(Default, ScenarioState)]
pub struct GitHubCredentialValidationState {
    /// Temporary directory backing the test key file.
    pub(crate) temp_dir: Slot<Arc<TempDir>>,
    /// Path to the key file under test.
    pub(crate) key_path: Slot<Utf8PathBuf>,
    /// The GitHub App ID to use.
    pub(crate) app_id: Slot<u64>,
    /// The mock API response type to simulate.
    pub(crate) mock_response: Slot<MockApiResponse>,
    /// Outcome of the most recent validation attempt.
    pub(crate) outcome: Slot<ValidationOutcome>,
}

/// Fixture providing fresh state for each scenario.
#[rstest::fixture]
pub fn github_credential_validation_state() -> GitHubCredentialValidationState {
    GitHubCredentialValidationState::default()
}
