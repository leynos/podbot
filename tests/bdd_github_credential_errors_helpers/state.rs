//! Scenario state for GitHub App credential error classification BDD
//! tests.

use std::sync::Arc;

use camino::Utf8PathBuf;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;
use tempfile::TempDir;

/// Convenience alias for step outcomes.
pub type StepResult<T> = Result<T, String>;

/// Type of mock HTTP response to simulate.
#[derive(Clone, Copy, Debug, Default)]
pub enum MockHttpResponse {
    /// HTTP 401 — credentials rejected.
    #[default]
    Unauthorized401,
    /// HTTP 403 — insufficient permissions.
    Forbidden403,
    /// HTTP 404 — App not found.
    NotFound404,
    /// HTTP 503 — server error.
    ServerError503,
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

/// State shared across GitHub credential error classification scenarios.
#[derive(Default, ScenarioState)]
pub struct GitHubCredentialErrorsState {
    /// Temporary directory backing the test key file.
    pub(crate) temp_dir: Slot<Arc<TempDir>>,
    /// Path to the key file under test.
    pub(crate) key_path: Slot<Utf8PathBuf>,
    /// The GitHub App ID to use.
    pub(crate) app_id: Slot<u64>,
    /// The mock HTTP response type to simulate.
    pub(crate) mock_response: Slot<MockHttpResponse>,
    /// Outcome of the most recent validation attempt.
    pub(crate) outcome: Slot<ValidationOutcome>,
}

/// Fixture providing fresh state for each scenario.
#[rstest::fixture]
pub fn github_credential_errors_state() -> GitHubCredentialErrorsState {
    GitHubCredentialErrorsState::default()
}
