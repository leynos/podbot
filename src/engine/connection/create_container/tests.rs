//! Unit tests for container-creation request mapping and error handling.

use std::sync::{Arc, Mutex};

use bollard::models::ContainerCreateResponse;
use mockall::mock;
use rstest::{fixture, rstest};

use super::*;
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
                let mut captured_locked = captured_for_closure
                    .lock()
                    .expect("mock capture lock should succeed");
                captured_locked.call_count += 1;
                captured_locked.options = options;
                captured_locked.body = Some(config);
            }

            let response = response_state_for_closure
                .lock()
                .expect("mock response lock should succeed")
                .take()
                .expect("mock response should be configured for the test");

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

fn take_options(captured: &Arc<Mutex<CapturedCreateCall>>) -> Option<CreateContainerOptions> {
    captured
        .lock()
        .expect("mock capture lock should succeed")
        .options
        .clone()
}

fn take_body(captured: &Arc<Mutex<CapturedCreateCall>>) -> Option<ContainerCreateBody> {
    captured
        .lock()
        .expect("mock capture lock should succeed")
        .body
        .clone()
}

fn call_count(captured: &Arc<Mutex<CapturedCreateCall>>) -> usize {
    captured
        .lock()
        .expect("mock capture lock should succeed")
        .call_count
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
fn from_sandbox_config_preserves_flags() {
    let sandbox = SandboxConfig {
        privileged: true,
        mount_dev_fuse: false,
    };

    let security = ContainerSecurityOptions::from_sandbox_config(&sandbox);

    assert!(security.privileged);
    assert!(!security.mount_dev_fuse);
    assert_eq!(security.selinux_label_mode, SelinuxLabelMode::KeepDefault);
}

#[rstest]
fn create_container_privileged_mode_has_minimal_overrides(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (creator, captured) = success_creator("container-id");
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions {
            privileged: true,
            mount_dev_fuse: true,
            selinux_label_mode: SelinuxLabelMode::KeepDefault,
        },
    )
    .map_err(|error| io_error(format!("request construction should succeed: {error}")))?
    .with_name(Some(String::from("podbot-test")));

    let container_id = runtime_handle
        .block_on(EngineConnector::create_container_async(&creator, &request))
        .map_err(|error| io_error(format!("container creation should succeed: {error}")))?;

    ensure(
        container_id == "container-id",
        format!("expected container-id, got {container_id}"),
    )?;
    ensure(call_count(&captured) == 1, "expected one engine call")?;

    let options =
        take_options(&captured).ok_or_else(|| io_error("create options should be captured"))?;
    ensure(
        options.name.as_deref() == Some("podbot-test"),
        "expected create options name podbot-test",
    )?;

    let body = take_body(&captured).ok_or_else(|| io_error("container body should be captured"))?;
    let host_config = body
        .host_config
        .ok_or_else(|| io_error("host config should be set"))?;
    ensure(
        host_config.privileged == Some(true),
        "expected privileged host config",
    )?;
    ensure(host_config.cap_add.is_none(), "did not expect cap_add")?;
    ensure(host_config.devices.is_none(), "did not expect devices")?;
    ensure(
        host_config.security_opt.is_none(),
        "did not expect security_opt",
    )
}

#[rstest]
fn create_container_minimal_mode_mounts_fuse(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (creator, captured) = success_creator("container-id");
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions::default(),
    )
    .map_err(|error| io_error(format!("request construction should succeed: {error}")))?;

    let _ = runtime_handle
        .block_on(EngineConnector::create_container_async(&creator, &request))
        .map_err(|error| io_error(format!("container creation should succeed: {error}")))?;

    let body = take_body(&captured).ok_or_else(|| io_error("container body should be captured"))?;
    let host_config = body
        .host_config
        .ok_or_else(|| io_error("host config should be set"))?;

    ensure(
        host_config.privileged == Some(false),
        "expected privileged=false for minimal mode",
    )?;
    ensure(
        host_config.cap_add == Some(vec![String::from("SYS_ADMIN")]),
        "expected SYS_ADMIN capability",
    )?;
    ensure(
        host_config.security_opt == Some(vec![String::from("label=disable")]),
        "expected label=disable security option",
    )?;

    let devices = host_config
        .devices
        .ok_or_else(|| io_error("/dev/fuse device should be mounted"))?;
    ensure(
        devices.len() == 1,
        format!("expected one /dev/fuse mapping, got {}", devices.len()),
    )?;
    let device = devices
        .first()
        .ok_or_else(|| io_error("`/dev/fuse` mapping should include one device"))?;
    ensure(
        device.path_on_host.as_deref() == Some("/dev/fuse"),
        "expected path_on_host /dev/fuse",
    )?;
    ensure(
        device.path_in_container.as_deref() == Some("/dev/fuse"),
        "expected path_in_container /dev/fuse",
    )?;
    ensure(
        device.cgroup_permissions.as_deref() == Some("rwm"),
        "expected /dev/fuse permissions rwm",
    )
}

#[rstest]
fn create_container_minimal_without_fuse_avoids_mount(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (creator, captured) = success_creator("container-id");
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions {
            privileged: false,
            mount_dev_fuse: false,
            selinux_label_mode: SelinuxLabelMode::DisableForContainer,
        },
    )
    .map_err(|error| io_error(format!("request construction should succeed: {error}")))?;

    let _ = runtime_handle
        .block_on(EngineConnector::create_container_async(&creator, &request))
        .map_err(|error| io_error(format!("container creation should succeed: {error}")))?;

    let body = take_body(&captured).ok_or_else(|| io_error("container body should be captured"))?;
    let host_config = body
        .host_config
        .ok_or_else(|| io_error("host config should be set"))?;

    ensure(
        host_config.privileged == Some(false),
        "expected privileged=false for minimal mode",
    )?;
    ensure(host_config.cap_add.is_none(), "did not expect cap_add")?;
    ensure(host_config.devices.is_none(), "did not expect devices")?;
    ensure(
        host_config.security_opt == Some(vec![String::from("label=disable")]),
        "expected label=disable security option",
    )
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
fn create_container_maps_engine_error(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (creator, _) = failing_creator(bollard::errors::Error::RequestTimeoutError);
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions::default(),
    )
    .map_err(|error| io_error(format!("request construction should succeed: {error}")))?;

    let result =
        runtime_handle.block_on(EngineConnector::create_container_async(&creator, &request));

    match result {
        Err(PodbotError::Container(ContainerError::CreateFailed { message }))
            if message.contains("Timeout error") =>
        {
            Ok(())
        }
        other => Err(io_error(format!(
            "expected create-failed timeout mapping, got: {other:?}"
        ))),
    }
}

#[rstest]
fn create_container_sync_uses_provided_runtime(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (creator, captured) = success_creator("container-id");
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions::default(),
    )
    .map_err(|error| io_error(format!("request construction should succeed: {error}")))?;

    let container_id =
        EngineConnector::create_container(runtime_handle.handle(), &creator, &request)
            .map_err(|error| io_error(format!("sync create should succeed: {error}")))?;

    ensure(
        container_id == "container-id",
        format!("expected container-id, got {container_id}"),
    )?;
    ensure(call_count(&captured) == 1, "expected one engine call")
}
