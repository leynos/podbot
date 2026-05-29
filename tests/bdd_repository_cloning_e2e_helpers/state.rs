//! Shared state and fixture for the end-to-end repository-cloning scenarios.

use std::sync::Arc;

use bollard::Docker;
use podbot::engine::RepositoryCloneResult;
use podbot::error::PodbotError;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;
use testcontainers::ContainerAsync;
use testcontainers::GenericImage;
use tokio::runtime::Runtime;

/// Step result type for the end-to-end repository-cloning scenarios.
pub type StepResult<T> = Result<T, String>;

/// Owned bundle of the sandbox container, its accompanying tokio runtime, and
/// the Bollard client pointed at the same socket.
///
/// `ContainerAsync<I>` uses `tokio::runtime::Handle::current()` inside its
/// asynchronous drop helper. Dropping the container outside an active tokio
/// runtime context therefore panics with "there is no reactor running". The
/// custom [`Drop`] implementation below explicitly tears the container down
/// via `runtime.block_on(...)` so the inner runtime is still live while
/// `ContainerAsync` releases its resources.
pub struct SandboxBundle {
    /// Tokio runtime kept alive for the lifetime of the scenario.
    pub runtime: Arc<Runtime>,
    /// The sandbox container; taken during [`Drop`] to release synchronously.
    pub container: Option<ContainerAsync<GenericImage>>,
    /// Bollard client used to drive the production exec path.
    pub docker: Arc<Docker>,
    /// Container identifier surfaced to step definitions.
    pub container_id: String,
}

impl Drop for SandboxBundle {
    fn drop(&mut self) {
        let Some(container) = self.container.take() else {
            return;
        };
        // Run the container teardown inside an active runtime task so the
        // `async_drop` helper inside `ContainerAsync` can resolve
        // `Handle::current()` and call `block_in_place` from a worker context.
        self.runtime.block_on(async move {
            drop(container.rm().await);
        });
    }
}

/// Shared scenario state for the end-to-end repository-cloning BDD tests.
#[derive(Default, ScenarioState)]
pub struct RepositoryCloningE2eState {
    /// Sandbox container guard kept alive for the duration of the scenario.
    pub(crate) bundle: Slot<Arc<SandboxBundle>>,
    /// `GIT_ASKPASS` helper path inside the container.
    pub(crate) askpass_path: Slot<String>,
    /// Absolute workspace path inside the container.
    pub(crate) workspace_base_dir: Slot<String>,
    /// Outcome of the most recent clone attempt.
    pub(crate) outcome: Slot<Result<RepositoryCloneResult, PodbotError>>,
}

/// Fixture providing fresh scenario state.
#[fixture]
pub fn repository_cloning_e2e_state() -> RepositoryCloningE2eState {
    RepositoryCloningE2eState::default()
}
