//! Given/when step definitions for container-creation behavioural scenarios.

use std::sync::{Arc, Mutex};

use bollard::models::{ContainerCreateBody, ContainerCreateResponse, HostConfig};
use bollard::query_parameters::CreateContainerOptions;
use mockall::mock;
use podbot::engine::{
    ContainerCreator, ContainerSecurityOptions, CreateContainerFuture, CreateContainerRequest,
    EngineConnector, SelinuxLabelMode,
};
use podbot::error::{ConfigError, ContainerError, PodbotError};
use rstest_bdd_macros::{given, when};

use super::state::{ContainerCreationState, CreateOutcome, FailureKind, StepResult};

mock! {
    #[derive(Debug)]
    Creator {}

    impl ContainerCreator for Creator {
        fn create_container<'a>(
            &'a self,
            options: Option<CreateContainerOptions>,
            config: ContainerCreateBody,
        ) -> CreateContainerFuture<'a>;
    }
}

#[given("a configured sandbox image {image}")]
fn configured_sandbox_image(container_creation_state: &ContainerCreationState, image: String) {
    container_creation_state.image.set(Some(image));
}

#[given("sandbox image is configured as whitespace only")]
fn sandbox_image_configured_as_whitespace_only(container_creation_state: &ContainerCreationState) {
    container_creation_state
        .image
        .set(Some(String::from("   ")));
}

#[given("no sandbox image is configured")]
fn no_sandbox_image_configured(container_creation_state: &ContainerCreationState) {
    container_creation_state.image.set(None);
}

#[given("sandbox security is privileged mode")]
fn sandbox_security_privileged_mode(container_creation_state: &ContainerCreationState) {
    container_creation_state
        .security
        .set(ContainerSecurityOptions {
            privileged: true,
            mount_dev_fuse: true,
            selinux_label_mode: SelinuxLabelMode::KeepDefault,
        });
}

#[given("sandbox security is minimal mode with /dev/fuse mounted")]
fn sandbox_security_minimal_with_fuse(container_creation_state: &ContainerCreationState) {
    container_creation_state
        .security
        .set(ContainerSecurityOptions::default());
}

#[given("sandbox security is minimal mode without /dev/fuse mounted")]
fn sandbox_security_minimal_without_fuse(container_creation_state: &ContainerCreationState) {
    container_creation_state
        .security
        .set(ContainerSecurityOptions {
            privileged: false,
            mount_dev_fuse: false,
            selinux_label_mode: SelinuxLabelMode::DisableForContainer,
        });
}

#[given("the container engine create call fails")]
fn container_engine_create_call_fails(container_creation_state: &ContainerCreationState) {
    container_creation_state.should_fail_create.set(true);
}

#[when("container creation is requested")]
fn container_creation_is_requested(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    let call_count = Arc::new(Mutex::new(0_usize));
    let captured_options = Arc::new(Mutex::new(None::<CreateContainerOptions>));
    let captured_host_config = Arc::new(Mutex::new(None::<HostConfig>));
    let should_fail = container_creation_state
        .should_fail_create
        .get()
        .unwrap_or(false);
    let creator = setup_mock_creator(
        should_fail,
        &call_count,
        &captured_options,
        &captured_host_config,
    );

    let request = match build_request_from_state(container_creation_state) {
        Ok(request) => request,
        Err(error) => {
            capture_mock_state(
                container_creation_state,
                &call_count,
                &captured_options,
                &captured_host_config,
            )?;
            record_failure(container_creation_state, &error);
            return Ok(());
        }
    };

    execute_and_capture_result(
        container_creation_state,
        &creator,
        &request,
        &call_count,
        &captured_options,
        &captured_host_config,
    )
}

fn setup_mock_creator(
    should_fail: bool,
    call_count: &Arc<Mutex<usize>>,
    captured_options: &Arc<Mutex<Option<CreateContainerOptions>>>,
    captured_host_config: &Arc<Mutex<Option<HostConfig>>>,
) -> MockCreator {
    let mut creator = MockCreator::new();
    let call_count_for_closure = Arc::clone(call_count);
    let captured_options_for_closure = Arc::clone(captured_options);
    let captured_host_config_for_closure = Arc::clone(captured_host_config);
    creator
        .expect_create_container()
        .returning(move |options, config| {
            if let Ok(mut locked) = call_count_for_closure.lock() {
                *locked += 1;
            }
            if let Ok(mut locked) = captured_options_for_closure.lock() {
                *locked = options;
            }
            if let Ok(mut locked) = captured_host_config_for_closure.lock() {
                *locked = config.host_config;
            }

            if should_fail {
                return Box::pin(async { Err(bollard::errors::Error::RequestTimeoutError) });
            }

            Box::pin(async {
                Ok(ContainerCreateResponse {
                    id: String::from("bdd-container-id"),
                    warnings: vec![],
                })
            })
        });

    creator
}

fn build_request_from_state(
    container_creation_state: &ContainerCreationState,
) -> Result<CreateContainerRequest, PodbotError> {
    let image = container_creation_state
        .image
        .get()
        .unwrap_or(None)
        .unwrap_or_default();
    let security = container_creation_state.security.get().unwrap_or_default();
    CreateContainerRequest::new(image, security)
        .map(|request| request.with_name(Some(String::from("podbot-sandbox-test"))))
}

#[expect(
    clippy::too_many_arguments,
    reason = "required by explicit refactor request for helper signature"
)]
fn execute_and_capture_result(
    container_creation_state: &ContainerCreationState,
    creator: &MockCreator,
    request: &CreateContainerRequest,
    call_count: &Arc<Mutex<usize>>,
    captured_options: &Arc<Mutex<Option<CreateContainerOptions>>>,
    captured_host_config: &Arc<Mutex<Option<HostConfig>>>,
) -> StepResult<()> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| String::from("failed to create tokio runtime for scenario"))?;
    let result = runtime.block_on(EngineConnector::create_container_async(creator, request));

    capture_mock_state(
        container_creation_state,
        call_count,
        captured_options,
        captured_host_config,
    )?;

    match result {
        Ok(container_id) => {
            container_creation_state
                .outcome
                .set(CreateOutcome::Success(container_id));
        }
        Err(error) => record_failure(container_creation_state, &error),
    }

    Ok(())
}

fn capture_mock_state(
    container_creation_state: &ContainerCreationState,
    call_count: &Arc<Mutex<usize>>,
    captured_options: &Arc<Mutex<Option<CreateContainerOptions>>>,
    captured_host_config: &Arc<Mutex<Option<HostConfig>>>,
) -> StepResult<()> {
    let call_count_value = *call_count
        .lock()
        .map_err(|_| String::from("engine call count mutex is poisoned"))?;
    let options_value = captured_options
        .lock()
        .map_err(|_| String::from("captured options mutex is poisoned"))?
        .clone();
    let host_config_value = captured_host_config
        .lock()
        .map_err(|_| String::from("captured host config mutex is poisoned"))?
        .clone();

    container_creation_state
        .engine_call_count
        .set(call_count_value);
    container_creation_state.captured_options.set(options_value);
    container_creation_state
        .captured_host_config
        .set(host_config_value);

    Ok(())
}

fn record_failure(container_creation_state: &ContainerCreationState, error: &PodbotError) {
    let kind = classify_failure_kind(error);
    container_creation_state.outcome.set(CreateOutcome::Failed {
        kind,
        message: error.to_string(),
    });
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
