//! Behavioural step helpers for container creation scenarios.

use std::sync::{Arc, Mutex};

use bollard::models::{ContainerCreateBody, ContainerCreateResponse, HostConfig};
use bollard::query_parameters::CreateContainerOptions;
use podbot::engine::{
    ContainerCreator, ContainerSecurityOptions, CreateContainerFuture, CreateContainerRequest,
    EngineConnector, SelinuxLabelMode,
};
use podbot::error::{ConfigError, ContainerError, PodbotError};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, then, when};

/// Step result type for container-creation BDD tests.
pub type StepResult<T> = Result<T, &'static str>;

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

/// Categorised failure outcomes for assertions.
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
    /// Configured image value used for request construction.
    image: Slot<Option<String>>,

    /// Security options used for request construction.
    security: Slot<ContainerSecurityOptions>,

    /// Whether the mocked engine should fail create calls.
    should_fail_create: Slot<bool>,

    /// Outcome of the most recent create attempt.
    outcome: Slot<CreateOutcome>,

    /// Captured create options forwarded to the engine.
    captured_options: Slot<Option<CreateContainerOptions>>,

    /// Captured host configuration forwarded to the engine.
    captured_host_config: Slot<Option<HostConfig>>,
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
    state
}

#[derive(Debug, Clone)]
struct RecordingCreator {
    state: Arc<Mutex<RecordingCreatorState>>,
}

#[derive(Debug)]
struct RecordingCreatorState {
    should_fail: bool,
    captured_options: Option<CreateContainerOptions>,
    captured_body: Option<ContainerCreateBody>,
}

impl RecordingCreator {
    fn new(should_fail: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingCreatorState {
                should_fail,
                captured_options: None,
                captured_body: None,
            })),
        }
    }

    fn capture_snapshot(&self) -> StepResult<(Option<CreateContainerOptions>, Option<HostConfig>)> {
        let mut locked = self
            .state
            .lock()
            .map_err(|_| "recording creator state mutex is poisoned")?;

        let options = locked.captured_options.take();
        let host_config = locked
            .captured_body
            .take()
            .and_then(|body| body.host_config);

        Ok((options, host_config))
    }
}

impl ContainerCreator for RecordingCreator {
    fn create_container(
        &self,
        options: Option<CreateContainerOptions>,
        config: ContainerCreateBody,
    ) -> CreateContainerFuture<'_> {
        let state = Arc::clone(&self.state);

        Box::pin(async move {
            let mut locked = state
                .lock()
                .map_err(|_| bollard::errors::Error::RequestTimeoutError)?;
            let should_fail = locked.should_fail;
            locked.captured_options = options;
            locked.captured_body = Some(config);

            if should_fail {
                return Err(bollard::errors::Error::RequestTimeoutError);
            }

            Ok(ContainerCreateResponse {
                id: String::from("bdd-container-id"),
                warnings: vec![],
            })
        })
    }
}

#[given("a configured sandbox image {image}")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step signatures consistently return StepResult"
)]
fn configured_sandbox_image(
    container_creation_state: &ContainerCreationState,
    image: String,
) -> StepResult<()> {
    container_creation_state.image.set(Some(image));
    Ok(())
}

#[given("no sandbox image is configured")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step signatures consistently return StepResult"
)]
fn no_sandbox_image_configured(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    container_creation_state.image.set(None);
    Ok(())
}

#[given("sandbox security is privileged mode")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step signatures consistently return StepResult"
)]
fn sandbox_security_privileged_mode(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    container_creation_state
        .security
        .set(ContainerSecurityOptions {
            privileged: true,
            mount_dev_fuse: true,
            selinux_label_mode: SelinuxLabelMode::KeepDefault,
        });
    Ok(())
}

#[given("sandbox security is minimal mode with /dev/fuse mounted")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step signatures consistently return StepResult"
)]
fn sandbox_security_minimal_with_fuse(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    container_creation_state
        .security
        .set(ContainerSecurityOptions::default());
    Ok(())
}

#[given("sandbox security is minimal mode without /dev/fuse mounted")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step signatures consistently return StepResult"
)]
fn sandbox_security_minimal_without_fuse(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    container_creation_state
        .security
        .set(ContainerSecurityOptions {
            privileged: false,
            mount_dev_fuse: false,
            selinux_label_mode: SelinuxLabelMode::DisableForContainer,
        });
    Ok(())
}

#[given("the container engine create call fails")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step signatures consistently return StepResult"
)]
fn container_engine_create_call_fails(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    container_creation_state.should_fail_create.set(true);
    Ok(())
}

#[when("container creation is requested")]
fn container_creation_is_requested(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    let creator = RecordingCreator::new(
        container_creation_state
            .should_fail_create
            .get()
            .unwrap_or(false),
    );

    let request = CreateContainerRequest {
        image: container_creation_state.image.get().unwrap_or(None),
        name: Some(String::from("podbot-sandbox-test")),
        cmd: None,
        env: None,
        security: container_creation_state.security.get().unwrap_or_default(),
    };

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| "failed to create tokio runtime for scenario")?;
    let result = runtime.block_on(EngineConnector::create_container_async(&creator, &request));

    let (captured_options, captured_host_config) = creator.capture_snapshot()?;
    container_creation_state
        .captured_options
        .set(captured_options);
    container_creation_state
        .captured_host_config
        .set(captured_host_config);

    match result {
        Ok(container_id) => {
            container_creation_state
                .outcome
                .set(CreateOutcome::Success(container_id));
        }
        Err(error) => {
            let kind = classify_failure_kind(&error);
            container_creation_state.outcome.set(CreateOutcome::Failed {
                kind,
                message: error.to_string(),
            });
        }
    }

    Ok(())
}

#[then("container creation succeeds")]
fn container_creation_succeeds(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    let outcome = container_creation_state
        .outcome
        .get()
        .ok_or("container creation outcome should be set")?;

    match outcome {
        CreateOutcome::Success(container_id) => {
            if container_id == "bdd-container-id" {
                return Ok(());
            }

            Err(Box::leak(
                format!("expected container id bdd-container-id, got {container_id}")
                    .into_boxed_str(),
            ))
        }
        CreateOutcome::Failed { message, .. } => Err(Box::leak(
            format!("expected success, got failure: {message}").into_boxed_str(),
        )),
    }
}

#[then("privileged host configuration is used")]
fn privileged_host_configuration_used(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    let host_config = captured_host_config(container_creation_state)?;

    if host_config.privileged != Some(true) {
        return Err("expected privileged host configuration");
    }
    if host_config.cap_add.is_some() {
        return Err("expected privileged host configuration without cap_add");
    }
    if host_config.devices.is_some() {
        return Err("expected privileged host configuration without device maps");
    }
    if host_config.security_opt.is_some() {
        return Err("expected privileged host configuration without security_opt");
    }

    let options = container_creation_state
        .captured_options
        .get()
        .flatten()
        .ok_or("create options should be captured")?;
    if options.name.as_deref() != Some("podbot-sandbox-test") {
        return Err("expected container name podbot-sandbox-test");
    }

    Ok(())
}

#[then("minimal host configuration with /dev/fuse is used")]
fn minimal_host_configuration_with_fuse_used(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    let host_config = captured_host_config(container_creation_state)?;

    if host_config.privileged != Some(false) {
        return Err("expected minimal host configuration with privileged=false");
    }
    if host_config.cap_add != Some(vec![String::from("SYS_ADMIN")]) {
        return Err("expected SYS_ADMIN capability in minimal mode with /dev/fuse");
    }
    if host_config.security_opt != Some(vec![String::from("label=disable")]) {
        return Err("expected SELinux label=disable in minimal mode");
    }

    let devices = host_config
        .devices
        .ok_or("minimal mode with fuse should map /dev/fuse")?;
    if devices.len() != 1 {
        return Err("expected exactly one /dev/fuse device mapping");
    }
    let device = devices
        .first()
        .ok_or("`/dev/fuse` mapping should include one device entry")?;
    if device.path_on_host.as_deref() != Some("/dev/fuse") {
        return Err("expected device path_on_host to be /dev/fuse");
    }
    if device.path_in_container.as_deref() != Some("/dev/fuse") {
        return Err("expected device path_in_container to be /dev/fuse");
    }
    if device.cgroup_permissions.as_deref() != Some("rwm") {
        return Err("expected /dev/fuse cgroup permissions to be rwm");
    }

    Ok(())
}

#[then("minimal host configuration without /dev/fuse is used")]
fn minimal_host_configuration_without_fuse_used(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    let host_config = captured_host_config(container_creation_state)?;

    if host_config.privileged != Some(false) {
        return Err("expected minimal host configuration with privileged=false");
    }
    if host_config.cap_add.is_some() {
        return Err("did not expect cap_add when /dev/fuse is disabled");
    }
    if host_config.devices.is_some() {
        return Err("did not expect device mappings when /dev/fuse is disabled");
    }
    if host_config.security_opt != Some(vec![String::from("label=disable")]) {
        return Err("expected SELinux label=disable in minimal mode");
    }

    Ok(())
}

#[then("container creation fails with missing image error")]
fn container_creation_fails_with_missing_image_error(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    assert_failure_kind(container_creation_state, FailureKind::MissingImage)
}

#[then("container creation fails with create failed error")]
fn container_creation_fails_with_create_failed_error(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    assert_failure_kind(container_creation_state, FailureKind::CreateFailed)
}

fn captured_host_config(
    container_creation_state: &ContainerCreationState,
) -> StepResult<HostConfig> {
    container_creation_state
        .captured_host_config
        .get()
        .flatten()
        .ok_or("captured host config should be available")
}

fn classify_failure_kind(error: &PodbotError) -> FailureKind {
    match error {
        PodbotError::Config(ConfigError::MissingRequired { field }) if field == "image" => {
            FailureKind::MissingImage
        }
        PodbotError::Container(ContainerError::CreateFailed { .. }) => FailureKind::CreateFailed,
        _ => FailureKind::Other,
    }
}

fn assert_failure_kind(
    container_creation_state: &ContainerCreationState,
    expected_kind: FailureKind,
) -> StepResult<()> {
    let outcome = container_creation_state
        .outcome
        .get()
        .ok_or("container creation outcome should be set")?;

    match outcome {
        CreateOutcome::Success(container_id) => Err(Box::leak(
            format!("expected failure, got success with container id: {container_id}")
                .into_boxed_str(),
        )),
        CreateOutcome::Failed { kind, message } => {
            if kind == expected_kind {
                return Ok(());
            }

            Err(Box::leak(
                format!("expected failure kind {expected_kind:?}, got {kind:?}: {message}")
                    .into_boxed_str(),
            ))
        }
    }
}
