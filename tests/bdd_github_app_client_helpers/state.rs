//! Scenario state for GitHub App client construction BDD tests.

use std::sync::Arc;

use camino::Utf8PathBuf;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;
use tempfile::TempDir;

/// Convenience alias for step outcomes.
pub type StepResult<T> = Result<T, String>;

/// Outcome of an App client build attempt.
#[derive(Clone, Debug)]
pub enum ClientBuildOutcome {
    /// The client was built successfully.
    Success,
    /// The build failed with the given error message.
    Failed {
        /// The `Display` representation of the error.
        message: String,
    },
}

/// State shared across GitHub App client construction scenarios.
#[derive(Default, ScenarioState)]
pub struct GitHubAppClientState {
    /// Temporary directory backing the test key file.
    pub(crate) temp_dir: Slot<Arc<TempDir>>,
    /// Path to the key file under test.
    pub(crate) key_path: Slot<Utf8PathBuf>,
    /// The GitHub App ID to use.
    pub(crate) app_id: Slot<u64>,
    /// Outcome of the most recent build attempt.
    pub(crate) outcome: Slot<ClientBuildOutcome>,
}

/// Fixture providing fresh state for each scenario.
#[rstest::fixture]
pub fn github_app_client_state() -> GitHubAppClientState {
    GitHubAppClientState::default()
}
