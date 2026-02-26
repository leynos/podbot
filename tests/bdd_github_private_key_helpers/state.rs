//! Scenario state for GitHub private key loading BDD tests.

use std::sync::Arc;

use camino::Utf8PathBuf;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;
use tempfile::TempDir;

/// Convenience alias for step outcomes.
pub type StepResult<T> = Result<T, String>;

/// Outcome of a private key load attempt.
#[derive(Clone, Debug)]
pub enum KeyLoadOutcome {
    /// The key loaded successfully.
    Success,
    /// The key load failed with the given error message.
    Failed {
        /// The `Display` representation of the error.
        message: String,
    },
}

/// State shared across GitHub private key loading scenarios.
#[derive(ScenarioState)]
pub struct GitHubPrivateKeyState {
    /// Temporary directory backing the test key file.
    pub(crate) temp_dir: Slot<Arc<TempDir>>,
    /// Path to the key file under test.
    pub(crate) key_path: Slot<Utf8PathBuf>,
    /// Outcome of the most recent load attempt.
    pub(crate) outcome: Slot<KeyLoadOutcome>,
}

#[expect(
    clippy::derivable_impls,
    reason = "ScenarioState guidance discourages deriving Default in this module"
)]
impl Default for GitHubPrivateKeyState {
    fn default() -> Self {
        Self {
            temp_dir: Slot::default(),
            key_path: Slot::default(),
            outcome: Slot::default(),
        }
    }
}

/// Fixture providing fresh state for each GitHub private key scenario.
#[rstest::fixture]
pub fn github_private_key_state() -> GitHubPrivateKeyState {
    GitHubPrivateKeyState::default()
}
