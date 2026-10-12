//! Edge-case unit tests for the minimal-mode container-creation path.

use rstest::rstest;

use super::*;

#[rstest]
#[case::with_fuse(true)]
#[case::without_fuse(false)]
fn from_sandbox_config_minimal_mode_passes_through_selinux_mode(#[case] fuse: bool) {
    let cfg = SandboxConfig {
        privileged: false,
        mount_dev_fuse: fuse,
        selinux_label_mode: SelinuxLabelMode::DisableForContainer,
    };
    let sec = ContainerSecurityOptions::from_sandbox_config(&cfg);
    assert!(!sec.privileged);
    assert_eq!(sec.mount_dev_fuse, fuse);
    assert_eq!(
        sec.selinux_label_mode,
        SelinuxLabelMode::DisableForContainer
    );
}

#[rstest]
fn from_sandbox_config_minimal_mode_with_keep_default() {
    let cfg = SandboxConfig {
        privileged: false,
        mount_dev_fuse: true,
        selinux_label_mode: SelinuxLabelMode::KeepDefault,
    };
    let sec = ContainerSecurityOptions::from_sandbox_config(&cfg);
    assert!(!sec.privileged);
    assert!(sec.mount_dev_fuse);
    assert_eq!(sec.selinux_label_mode, SelinuxLabelMode::KeepDefault);
}

/// Run a minimal-mode container creation with the given fuse and `SELinux`
/// settings, returning the captured options and body after validating the
/// basic minimal-mode invariants (privileged is false).
fn minimal_create(
    rt: &tokio::runtime::Runtime,
    fuse: bool,
    selinux: SelinuxLabelMode,
) -> std::io::Result<(Option<CreateContainerOptions>, ContainerCreateBody)> {
    let sec = ContainerSecurityOptions {
        privileged: false,
        mount_dev_fuse: fuse,
        selinux_label_mode: selinux,
    };
    let (creator, captured) = success_creator("container-id");
    let req = CreateContainerRequest::new("ghcr.io/example/sandbox:latest", sec)
        .map_err(|e| io_error(format!("request construction should succeed: {e}")))?;
    let _ = rt
        .block_on(EngineConnector::create_container_async(&creator, &req))
        .map_err(|e| io_error(format!("container creation should succeed: {e}")))?;
    let body = clone_captured_body(&captured)
        .ok_or_else(|| io_error("container body should be captured"))?;
    let host_config = body
        .host_config
        .as_ref()
        .ok_or_else(|| io_error("host config should be set"))?;
    ensure(
        host_config.privileged == Some(false),
        "expected privileged=false for minimal mode",
    )?;
    Ok((clone_captured_options(&captured), body))
}

#[rstest]
fn create_container_minimal_mode_with_selinux_keep_default(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) {
    let runtime_handle = runtime.expect("tokio runtime should be created");
    let (_, body) = minimal_create(&runtime_handle, true, SelinuxLabelMode::KeepDefault)
        .expect("minimal-mode creation should succeed");
    let host_config = body
        .host_config
        .as_ref()
        .expect("host config should be set");
    assert!(
        host_config.security_opt.is_none(),
        "did not expect security_opt with KeepDefault"
    );
    assert!(
        host_config.cap_add == Some(vec![String::from("SYS_ADMIN")]),
        "expected SYS_ADMIN capability for fuse mount"
    );
    assert!(
        host_config.devices.is_some(),
        "expected /dev/fuse device to be mounted"
    );
}

#[rstest]
fn create_container_minimal_mode_without_fuse_omits_capabilities(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) {
    let runtime_handle = runtime.expect("tokio runtime should be created");
    let (_, body) = minimal_create(
        &runtime_handle,
        false,
        SelinuxLabelMode::DisableForContainer,
    )
    .expect("minimal-mode creation should succeed");
    let host_config = body
        .host_config
        .as_ref()
        .expect("host config should be set");
    assert!(host_config.cap_add.is_none(), "did not expect cap_add");
    assert!(host_config.devices.is_none(), "did not expect devices");
    assert!(
        host_config.security_opt == Some(vec![String::from("label=disable")]),
        "expected label=disable security option in minimal mode without fuse"
    );
}

#[rstest]
fn create_container_minimal_mode_with_fuse_verifies_device_details(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) {
    let runtime_handle = runtime.expect("tokio runtime should be created");
    let (_, body) = minimal_create(&runtime_handle, true, SelinuxLabelMode::DisableForContainer)
        .expect("minimal-mode creation should succeed");
    let host_config = body
        .host_config
        .as_ref()
        .expect("host config should be set");
    let devices = host_config
        .devices
        .as_ref()
        .expect("/dev/fuse device should be mounted");
    assert!(
        devices.len() == 1,
        "expected one /dev/fuse mapping, got {}",
        devices.len()
    );
    let device = devices
        .first()
        .expect("/dev/fuse mapping should include one device");
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
fn create_container_minimal_mode_without_optional_fields(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) {
    let runtime_handle = runtime.expect("tokio runtime should be created");
    let (opts, body) = minimal_create(&runtime_handle, true, SelinuxLabelMode::DisableForContainer)
        .expect("minimal-mode creation should succeed");
    assert!(opts.is_none(), "expected no create options");
    assert!(body.image.is_some(), "expected image to be set");
    assert!(body.cmd.is_none(), "did not expect cmd");
    assert!(body.env.is_none(), "did not expect env");
}
