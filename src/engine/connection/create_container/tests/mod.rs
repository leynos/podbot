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
use crate::config::{AppConfig, SandboxConfig};
use crate::error::{ConfigError, ContainerError, PodbotError};

fn io_error(message: impl Into<String>) -> std::io::Error {
    std::io::Error::other(message.into())
}

fn ensure(condition: bool, message: impl Into<String>) -> std::io::Result<()> {
    if condition {
        return Ok(());
    }

    Err(io_error(message))
}

#[rstest]
fn create_container_maps_labels_mounts_and_working_dir(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) {
    use std::collections::HashMap;

    use bollard::models::MountTypeEnum;

    let runtime_handle = runtime.expect("tokio runtime should be created");
    let (creator, captured) = success_creator("container-id");
    let request = CreateContainerRequest::new(
        "ghcr.io/example/sandbox:latest",
        ContainerSecurityOptions::default(),
    )
    .expect("request construction should succeed")
    .with_labels(vec![(
        String::from(crate::engine::MANAGED_LABEL_KEY),
        String::from(crate::engine::MANAGED_LABEL_VALUE),
    )])
    .with_mounts(vec![
        BindMount::read_only(
            "/run/user/1000/podbot/session/ghapp_token",
            "/run/secrets/podbot",
        )
        .expect("read-only bind mount should validate"),
    ])
    .with_working_dir(Some(String::from("/workspace/owner/name")));

    let _ = runtime_handle
        .block_on(EngineConnector::create_container_async(&creator, &request))
        .expect("container creation should succeed");

    let body = clone_captured_body(&captured).expect("container body should be captured");

    assert_eq!(
        body.labels,
        Some(HashMap::from([(
            String::from("io.podbot.managed"),
            String::from("true"),
        )])),
        "expected ownership labels in the create body"
    );
    assert_eq!(
        body.working_dir.as_deref(),
        Some("/workspace/owner/name"),
        "expected the configured working directory"
    );

    let host_config = body.host_config.expect("host config should be set");
    let mounts = host_config.mounts.expect("expected one bind mount");
    assert_eq!(mounts.len(), 1, "expected exactly one mount");
    let mount = mounts.first().expect("expected the bind mount");
    assert_eq!(mount.typ, Some(MountTypeEnum::BIND));
    assert_eq!(
        mount.source.as_deref(),
        Some("/run/user/1000/podbot/session/ghapp_token")
    );
    assert_eq!(mount.target.as_deref(), Some("/run/secrets/podbot"));
    assert_eq!(mount.read_only, Some(true));
}

#[rstest]
fn create_container_without_metadata_omits_optional_payload(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) {
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
    assert!(body.labels.is_none(), "did not expect labels");
    assert!(body.working_dir.is_none(), "did not expect a working dir");

    let host_config = body.host_config.expect("host config should be set");
    assert!(host_config.mounts.is_none(), "did not expect mounts");
}

#[rstest]
fn bind_mount_rejects_relative_paths() {
    let result = BindMount::read_only("relative/token", "/run/secrets/podbot");

    assert!(
        matches!(
            result,
            Err(PodbotError::Config(ConfigError::InvalidValue { ref field, .. }))
                if field == "mount.source"
        ),
        "expected relative source rejection, got {result:?}"
    );

    let write_result = BindMount::read_write("/run/user/1000/token", "run/secrets/podbot");

    assert!(
        matches!(
            write_result,
            Err(PodbotError::Config(ConfigError::InvalidValue { ref field, .. }))
                if field == "mount.target"
        ),
        "expected relative target rejection, got {write_result:?}"
    );
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
