//! Unit tests for container-creation request mapping and error handling.

mod minimal_mode;
mod privileged_mode;
mod request_construction;

use std::sync::{Arc, Mutex};

use bollard::models::ContainerCreateResponse;
use mockall::mock;
use rstest::{fixture, rstest};

use super::*;
use crate::config::AppConfig;
use crate::error::{ConfigError, ContainerError};

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

#[derive(Debug, Default)]
struct CapturedCreateCall {
    call_count: usize,
    options: Option<CreateContainerOptions>,
    body: Option<ContainerCreateBody>,
}

/// Error returned when a mock is invoked more often than it was primed for.
fn mock_not_configured_error() -> bollard::errors::Error {
    bollard::errors::Error::IOError {
        err: std::io::Error::other("mock response not configured for this call"),
    }
}

fn creator_with_result(
    result: Result<ContainerCreateResponse, bollard::errors::Error>,
) -> (MockCreator, Arc<Mutex<CapturedCreateCall>>) {
    let mut creator = MockCreator::new();
    let captured = Arc::new(Mutex::new(CapturedCreateCall::default()));
    let captured_for_closure = Arc::clone(&captured);
    let response_state = Arc::new(Mutex::new(Some(result)));
    let response_state_for_closure = Arc::clone(&response_state);

    creator
        .expect_create_container()
        .returning(move |options, config| {
            {
                // Recovering the guard keeps the captured call readable after a
                // panicking test; the data remains structurally valid.
                let mut captured_locked = captured_for_closure
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                captured_locked.call_count += 1;
                captured_locked.options = options;
                captured_locked.body = Some(config);
            }

            // Surface an unexpected second call through the mock's own error
            // channel so the test fails on its assertions, not inside the mock.
            let response = response_state_for_closure
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .unwrap_or_else(|| Err(mock_not_configured_error()));

            Box::pin(async move { response })
        });

    (creator, captured)
}

fn success_creator(container_id: &str) -> (MockCreator, Arc<Mutex<CapturedCreateCall>>) {
    creator_with_result(Ok(ContainerCreateResponse {
        id: String::from(container_id),
        warnings: vec![],
    }))
}

fn failing_creator(error: bollard::errors::Error) -> (MockCreator, Arc<Mutex<CapturedCreateCall>>) {
    creator_with_result(Err(error))
}

/// Reads the captured call, recovering from a poisoned mutex because the
/// captured data stays valid even when a test panics while holding the lock.
fn captured_call(
    captured: &Arc<Mutex<CapturedCreateCall>>,
) -> std::sync::MutexGuard<'_, CapturedCreateCall> {
    captured
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn clone_captured_options(
    captured: &Arc<Mutex<CapturedCreateCall>>,
) -> Option<CreateContainerOptions> {
    captured_call(captured).options.clone()
}

fn clone_captured_body(captured: &Arc<Mutex<CapturedCreateCall>>) -> Option<ContainerCreateBody> {
    captured_call(captured).body.clone()
}

fn call_count(captured: &Arc<Mutex<CapturedCreateCall>>) -> usize {
    captured_call(captured).call_count
}

fn io_error(message: impl Into<String>) -> std::io::Error {
    std::io::Error::other(message.into())
}

fn ensure(condition: bool, message: impl Into<String>) -> std::io::Result<()> {
    if condition {
        return Ok(());
    }

    Err(io_error(message))
}

#[fixture]
fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Runtime::new()
}

#[rstest]
fn create_container_privileged_mode_has_minimal_overrides(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) {
    let runtime_handle = runtime.expect("tokio runtime should be created");
    let (creator, captured) = success_creator("container-id");
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions {
            privileged: true,
            mount_dev_fuse: true,
            selinux_label_mode: SelinuxLabelMode::KeepDefault,
        },
    )
    .expect("request construction should succeed")
    .with_name(Some(String::from("podbot-test")));

    let container_id = runtime_handle
        .block_on(EngineConnector::create_container_async(&creator, &request))
        .expect("container creation should succeed");

    assert!(
        container_id == "container-id",
        "expected container-id, got {container_id}"
    );
    assert!(call_count(&captured) == 1, "expected one engine call");

    let options = clone_captured_options(&captured).expect("create options should be captured");
    assert!(
        options.name.as_deref() == Some("podbot-test"),
        "expected create options name podbot-test"
    );

    let body = clone_captured_body(&captured).expect("container body should be captured");
    let host_config = body.host_config.expect("host config should be set");
    assert!(
        host_config.privileged == Some(true),
        "expected privileged host config"
    );
    assert!(host_config.cap_add.is_none(), "did not expect cap_add");
    assert!(host_config.devices.is_none(), "did not expect devices");
    assert!(
        host_config.security_opt.is_none(),
        "did not expect security_opt"
    );
}

#[rstest]
fn create_container_minimal_mode_mounts_fuse(runtime: std::io::Result<tokio::runtime::Runtime>) {
    let runtime_handle = runtime.expect("tokio runtime should be created");
    let (creator, captured) = success_creator("container-id");
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions::default(),
    )
    .expect("request construction should succeed");

    let _ = runtime_handle
        .block_on(EngineConnector::create_container_async(&creator, &request))
        .expect("container creation should succeed");

    let body = clone_captured_body(&captured).expect("container body should be captured");
    let host_config = body.host_config.expect("host config should be set");

    assert!(
        host_config.privileged == Some(false),
        "expected privileged=false for minimal mode"
    );
    assert!(
        host_config.cap_add == Some(vec![String::from("SYS_ADMIN")]),
        "expected SYS_ADMIN capability"
    );
    assert!(
        host_config.security_opt == Some(vec![String::from("label=disable")]),
        "expected label=disable security option"
    );

    let devices = host_config
        .devices
        .expect("/dev/fuse device should be mounted");
    assert!(
        devices.len() == 1,
        "expected one /dev/fuse mapping, got {}",
        devices.len()
    );
    let device = devices
        .first()
        .expect("`/dev/fuse` mapping should include one device");
    assert!(
        device.path_on_host.as_deref() == Some("/dev/fuse"),
        "expected path_on_host /dev/fuse"
    );
    assert!(
        device.path_in_container.as_deref() == Some("/dev/fuse"),
        "expected path_in_container /dev/fuse"
    );
    assert!(
        device.cgroup_permissions.as_deref() == Some("rwm"),
        "expected /dev/fuse permissions rwm"
    );
}

#[rstest]
fn create_container_minimal_without_fuse_avoids_mount(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) {
    let runtime_handle = runtime.expect("tokio runtime should be created");
    let (creator, captured) = success_creator("container-id");
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions {
            privileged: false,
            mount_dev_fuse: false,
            selinux_label_mode: SelinuxLabelMode::DisableForContainer,
        },
    )
    .expect("request construction should succeed");

    let _ = runtime_handle
        .block_on(EngineConnector::create_container_async(&creator, &request))
        .expect("container creation should succeed");

    let body = clone_captured_body(&captured).expect("container body should be captured");
    let host_config = body.host_config.expect("host config should be set");

    assert!(
        host_config.privileged == Some(false),
        "expected privileged=false for minimal mode"
    );
    assert!(host_config.cap_add.is_none(), "did not expect cap_add");
    assert!(host_config.devices.is_none(), "did not expect devices");
    assert!(
        host_config.security_opt == Some(vec![String::from("label=disable")]),
        "expected label=disable security option"
    );
}

#[rstest]
fn create_container_requires_image() {
    let (creator, captured) = success_creator("container-id");
    let request = CreateContainerRequest::new("   ", ContainerSecurityOptions::default());

    assert!(
        matches!(
            request,
            Err(PodbotError::Config(ConfigError::MissingRequired { ref field }))
                if field == "image"
        ),
        "expected missing image validation error, got: {request:?}"
    );
    assert_eq!(call_count(&captured), 0);
    let _ = creator;
}

#[rstest]
fn create_container_maps_engine_error(runtime: std::io::Result<tokio::runtime::Runtime>) {
    let runtime_handle = runtime.expect("tokio runtime should be created");
    let (creator, _) = failing_creator(bollard::errors::Error::RequestTimeoutError);
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions::default(),
    )
    .expect("request construction should succeed");

    let result =
        runtime_handle.block_on(EngineConnector::create_container_async(&creator, &request));

    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::CreateFailed { ref message }))
                if message.contains("Timeout error")
        ),
        "expected create-failed timeout mapping, got: {result:?}"
    );
}

#[rstest]
fn create_container_sync_uses_provided_runtime(runtime: std::io::Result<tokio::runtime::Runtime>) {
    let runtime_handle = runtime.expect("tokio runtime should be created");
    let (creator, captured) = success_creator("container-id");
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions::default(),
    )
    .expect("request construction should succeed");

    let container_id =
        EngineConnector::create_container(runtime_handle.handle(), &creator, &request)
            .expect("sync create should succeed");

    assert!(
        container_id == "container-id",
        "expected container-id, got {container_id}"
    );
    assert!(call_count(&captured) == 1, "expected one engine call");
}
