//! Edge-case unit tests for the privileged-mode container-creation path.

use rstest::rstest;

use super::*;

#[rstest]
fn from_sandbox_config_non_privileged_sets_selinux_disable() {
    let cfg = SandboxConfig {
        privileged: false,
        mount_dev_fuse: true,
    };
    let sec = ContainerSecurityOptions::from_sandbox_config(&cfg);
    assert_eq!(
        sec.selinux_label_mode,
        SelinuxLabelMode::DisableForContainer
    );
}

fn privileged_create(
    rt: &tokio::runtime::Runtime,
    fuse: bool,
    selinux: SelinuxLabelMode,
) -> std::io::Result<(Option<CreateContainerOptions>, ContainerCreateBody)> {
    let sec = ContainerSecurityOptions {
        privileged: true,
        mount_dev_fuse: fuse,
        selinux_label_mode: selinux,
    };
    let (creator, captured) = success_creator("container-id");
    let req = CreateContainerRequest::new("ghcr.io/example/sandbox:latest", sec)
        .map_err(|e| io_error(format!("request construction should succeed: {e}")))?;
    let _ = rt
        .block_on(EngineConnector::create_container_async(&creator, &req))
        .map_err(|e| io_error(format!("container creation should succeed: {e}")))?;
    let body = take_body(&captured).ok_or_else(|| io_error("container body should be captured"))?;
    let host_config = body
        .host_config
        .as_ref()
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
    )?;
    Ok((take_options(&captured), body))
}

#[rstest]
#[case::ignores_fuse_disabled(false, SelinuxLabelMode::KeepDefault)]
#[case::ignores_selinux_override(true, SelinuxLabelMode::DisableForContainer)]
fn create_container_privileged_mode_ignores_irrelevant_settings(
    runtime: std::io::Result<tokio::runtime::Runtime>,
    #[case] fuse: bool,
    #[case] selinux: SelinuxLabelMode,
) -> std::io::Result<()> {
    privileged_create(&runtime?, fuse, selinux).map(|_| ())
}

#[rstest]
fn create_container_privileged_mode_without_optional_fields(
    runtime: std::io::Result<tokio::runtime::Runtime>,
) -> std::io::Result<()> {
    let (opts, body) = privileged_create(&runtime?, true, SelinuxLabelMode::KeepDefault)?;
    ensure(opts.is_none(), "expected no create options")?;
    ensure(body.image.is_some(), "expected image to be set")?;
    ensure(body.cmd.is_none(), "did not expect cmd")?;
    ensure(body.env.is_none(), "did not expect env")
}
