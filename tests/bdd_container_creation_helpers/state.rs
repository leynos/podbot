//! Shared behavioural-test state for container-creation scenarios.

use bollard::models::HostConfig;
use bollard::query_parameters::CreateContainerOptions;
use podbot::engine::ContainerSecurityOptions;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;

/// Step result type for container-creation BDD tests.
pub type StepResult<T> = Result<T, String>;

/// High-level outcome observed after a container-creation attempt.
#[derive(Clone)]
pub enum CreateOutcome {
    /// Container creation succeeded and returned an ID.
    Success(String),

    /// Container creation failed with a classified failure kind and message.
    Failed {
        /// The failure category.
        kind: FailureKind,
        /// Human-readable error message.
        message: String,
    },
}

/// Categorized failure outcomes for assertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Required image configuration was missing.
    MissingImage,
    /// Engine rejected the create request.
    CreateFailed,
    /// Any other failure kind.
    Other,
}

/// Shared scenario state for container-creation behavioural tests.
#[derive(Default, ScenarioState)]
pub struct ContainerCreationState {
    /// Resolved configuration image value used for request construction.
    pub(crate) image: Slot<Option<String>>,

    /// Security options used for request construction.
    pub(crate) security: Slot<ContainerSecurityOptions>,

    /// Whether the mocked engine should fail create calls.
    pub(crate) should_fail_create: Slot<bool>,

    /// Outcome of the most recent create attempt.
    pub(crate) outcome: Slot<CreateOutcome>,

    /// Captured create options forwarded to the engine.
    pub(crate) captured_options: Slot<Option<CreateContainerOptions>>,

    /// Captured host configuration forwarded to the engine.
    pub(crate) captured_host_config: Slot<Option<HostConfig>>,

    /// Captured image field forwarded to the engine.
    pub(crate) captured_image: Slot<Option<String>>,

    /// Number of times the mocked engine create operation was invoked.
    pub(crate) engine_call_count: Slot<usize>,
}

/// Fixture providing fresh state for each container-creation scenario.
#[fixture]
pub fn container_creation_state() -> ContainerCreationState {
    let state = ContainerCreationState::default();
    state
        .image
        .set(Some(String::from("ghcr.io/example/podbot-sandbox:latest")));
    state.security.set(ContainerSecurityOptions::default());
    state.should_fail_create.set(false);
    state.engine_call_count.set(0);
    state
}
