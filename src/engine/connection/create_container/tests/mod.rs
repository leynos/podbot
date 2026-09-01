//! Unit tests for container-creation request mapping and error handling.

mod doubles;
mod minimal_mode;
mod privileged_mode;
mod request_construction;

use rstest::{fixture, rstest};

use self::doubles::{
    call_count, clone_captured_body, clone_captured_options, failing_creator, success_creator,
};
use super::*;
use crate::config::AppConfig;
use crate::error::{ConfigError, ContainerError};

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
