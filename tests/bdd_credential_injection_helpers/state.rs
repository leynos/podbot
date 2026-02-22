//! Shared behavioural-test state for credential-injection scenarios.

use std::sync::Arc;

use camino::Utf8PathBuf;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;
use tempfile::TempDir;

/// Step result type for credential-injection BDD tests.
pub type StepResult<T> = Result<T, String>;

/// Temporary host-home directory used by a scenario.
#[derive(Clone)]
pub struct HostHome {
    /// Keeps the temporary directory alive for the full scenario.
    pub(crate) _temp_dir: Arc<TempDir>,

    /// UTF-8 path to the host-home directory.
    pub(crate) path: Utf8PathBuf,
}

impl HostHome {
    /// Create a new temporary host-home directory for a scenario.
    pub(crate) fn new() -> StepResult<Self> {
        let temp_dir = tempfile::tempdir()
            .map_err(|error| format!("failed to create temporary host home directory: {error}"))?;

        let utf8_path =
            Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).map_err(|_| {
                String::from("temporary host home directory path should be valid UTF-8")
            })?;

        Ok(Self {
            _temp_dir: Arc::new(temp_dir),
            path: utf8_path,
        })
    }
}

/// High-level outcome observed after a credential-injection attempt.
#[derive(Clone)]
pub enum InjectionOutcome {
    /// Injection succeeded with reported credential paths.
    Success(Vec<String>),

    /// Injection failed with a classified failure kind.
    Failed {
        /// The failure category.
        kind: FailureKind,
        /// Human-readable error message.
        message: String,
        /// Container identifier extracted from the error, if available.
        container_id: Option<String>,
    },
}

/// Categorized failure outcomes for assertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Credential upload failed in the engine layer.
    UploadFailed,
    /// Any non-upload failure.
    Other,
}

/// Shared scenario state for credential-injection behavioural tests.
#[derive(Default, ScenarioState)]
pub struct CredentialInjectionState {
    /// Target container identifier used when building the upload request.
    pub(crate) container_id: Slot<String>,

    /// Scenario-scoped host-home directory with test credentials.
    pub(crate) host_home: Slot<HostHome>,

    /// Whether `~/.claude` should be selected for upload.
    pub(crate) copy_claude: Slot<bool>,

    /// Whether `~/.codex` should be selected for upload.
    pub(crate) copy_codex: Slot<bool>,

    /// Whether the mocked upload transport should fail.
    pub(crate) should_fail_upload: Slot<bool>,

    /// Outcome of the most recent credential-injection attempt.
    pub(crate) outcome: Slot<InjectionOutcome>,

    /// Number of times the mocked upload transport was invoked.
    pub(crate) upload_call_count: Slot<usize>,
}

/// Fixture providing fresh state for each credential-injection scenario.
#[fixture]
pub fn credential_injection_state() -> CredentialInjectionState {
    let state = CredentialInjectionState::default();
    state
        .container_id
        .set(String::from("sandbox-credential-test"));
    state.copy_claude.set(true);
    state.copy_codex.set(true);
    state.should_fail_upload.set(false);
    state.upload_call_count.set(0);
    state
}
