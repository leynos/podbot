//! Then-step assertions for container-creation behavioural scenarios.

use bollard::models::HostConfig;
use rstest_bdd_macros::then;

use super::state::{ContainerCreationState, CreateOutcome, FailureKind, StepResult};

#[then("container creation succeeds")]
fn container_creation_succeeds(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    let outcome = container_creation_state
        .outcome
        .get()
        .ok_or_else(|| String::from("container creation outcome should be set"))?;

    match outcome {
        CreateOutcome::Success(container_id) => {
            if container_id == "bdd-container-id" {
                return Ok(());
            }

            Err(format!(
                "expected container id bdd-container-id, got {container_id}"
            ))
        }
        CreateOutcome::Failed { message, .. } => {
            Err(format!("expected success, got failure: {message}"))
        }
    }
}

#[then("privileged host configuration is used")]
fn privileged_host_configuration_used(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    let host_config = captured_host_config(container_creation_state)?;

    if host_config.privileged != Some(true) {
        return Err(String::from("expected privileged host configuration"));
    }
    if host_config.cap_add.is_some() {
        return Err(String::from(
            "expected privileged host configuration without cap_add",
        ));
    }
    if host_config.devices.is_some() {
        return Err(String::from(
            "expected privileged host configuration without device maps",
        ));
    }
    if host_config.security_opt.is_some() {
        return Err(String::from(
            "expected privileged host configuration without security_opt",
        ));
    }

    let options = container_creation_state
        .captured_options
        .get()
        .flatten()
        .ok_or_else(|| String::from("create options should be captured"))?;
    if options.name.as_deref() != Some("podbot-sandbox-test") {
        return Err(String::from("expected container name podbot-sandbox-test"));
    }

    Ok(())
}

#[then("minimal host configuration with /dev/fuse is used")]
fn minimal_host_configuration_with_fuse_used(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    let host_config = captured_host_config(container_creation_state)?;

    if host_config.privileged != Some(false) {
        return Err(String::from(
            "expected minimal host configuration with privileged=false",
        ));
    }
    if host_config.cap_add != Some(vec![String::from("SYS_ADMIN")]) {
        return Err(String::from(
            "expected SYS_ADMIN capability in minimal mode with /dev/fuse",
        ));
    }
    if host_config.security_opt != Some(vec![String::from("label=disable")]) {
        return Err(String::from(
            "expected SELinux label=disable in minimal mode",
        ));
    }

    let devices = host_config
        .devices
        .ok_or_else(|| String::from("minimal mode with fuse should map /dev/fuse"))?;
    if devices.len() != 1 {
        return Err(String::from(
            "expected exactly one /dev/fuse device mapping",
        ));
    }
    let device = devices
        .first()
        .ok_or_else(|| String::from("`/dev/fuse` mapping should include one device entry"))?;
    if device.path_on_host.as_deref() != Some("/dev/fuse") {
        return Err(String::from("expected device path_on_host to be /dev/fuse"));
    }
    if device.path_in_container.as_deref() != Some("/dev/fuse") {
        return Err(String::from(
            "expected device path_in_container to be /dev/fuse",
        ));
    }
    if device.cgroup_permissions.as_deref() != Some("rwm") {
        return Err(String::from(
            "expected /dev/fuse cgroup permissions to be rwm",
        ));
    }

    Ok(())
}

#[then("minimal host configuration without /dev/fuse is used")]
fn minimal_host_configuration_without_fuse_used(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    let host_config = captured_host_config(container_creation_state)?;

    if host_config.privileged != Some(false) {
        return Err(String::from(
            "expected minimal host configuration with privileged=false",
        ));
    }
    if host_config.cap_add.is_some() {
        return Err(String::from(
            "did not expect cap_add when /dev/fuse is disabled",
        ));
    }
    if host_config.devices.is_some() {
        return Err(String::from(
            "did not expect device mappings when /dev/fuse is disabled",
        ));
    }
    if host_config.security_opt != Some(vec![String::from("label=disable")]) {
        return Err(String::from(
            "expected SELinux label=disable in minimal mode",
        ));
    }

    Ok(())
}

#[then("container creation fails with missing image error")]
fn container_creation_fails_with_missing_image_error(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    assert_failure_kind(container_creation_state, FailureKind::MissingImage)?;
    assert_engine_not_called(container_creation_state)
}

#[then("container creation fails with create failed error")]
fn container_creation_fails_with_create_failed_error(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    assert_failure_kind(container_creation_state, FailureKind::CreateFailed)
}

#[then("container engine is not invoked")]
fn container_engine_is_not_invoked(
    container_creation_state: &ContainerCreationState,
) -> StepResult<()> {
    assert_engine_not_called(container_creation_state)
}

fn captured_host_config(
    container_creation_state: &ContainerCreationState,
) -> StepResult<HostConfig> {
    container_creation_state
        .captured_host_config
        .get()
        .flatten()
        .ok_or_else(|| String::from("captured host config should be available"))
}

fn assert_failure_kind(
    container_creation_state: &ContainerCreationState,
    expected_kind: FailureKind,
) -> StepResult<()> {
    let outcome = container_creation_state
        .outcome
        .get()
        .ok_or_else(|| String::from("container creation outcome should be set"))?;

    match outcome {
        CreateOutcome::Success(container_id) => Err(format!(
            "expected failure, got success with container id: {container_id}"
        )),
        CreateOutcome::Failed { kind, message } => {
            if kind == expected_kind {
                return Ok(());
            }

            Err(format!(
                "expected failure kind {expected_kind:?}, got {kind:?}: {message}"
            ))
        }
    }
}

fn assert_engine_not_called(container_creation_state: &ContainerCreationState) -> StepResult<()> {
    let engine_call_count = container_creation_state
        .engine_call_count
        .get()
        .ok_or_else(|| String::from("engine call count should be captured"))?;

    if engine_call_count == 0 {
        return Ok(());
    }

    Err(format!(
        "engine should not be called when request validation fails, got {engine_call_count} calls"
    ))
}
