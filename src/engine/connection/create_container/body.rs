//! `Bollard` payload construction for container creation.
//!
//! These helpers translate a validated [`CreateContainerRequest`] into the
//! query options and body payload expected by the container engine.

use std::collections::HashMap;

use bollard::models::{ContainerCreateBody, DeviceMapping, HostConfig, Mount, MountTypeEnum};
use bollard::query_parameters::{CreateContainerOptions, CreateContainerOptionsBuilder};

use super::ContainerSecurityOptions;
use super::request::CreateContainerRequest;

const DEV_FUSE_PATH: &str = "/dev/fuse";
const FUSE_DEVICE_PERMISSIONS: &str = "rwm";
const CAP_SYS_ADMIN: &str = "SYS_ADMIN";
const SELINUX_LABEL_DISABLE: &str = "label=disable";

pub(super) fn build_create_options(name: Option<&str>) -> Option<CreateContainerOptions> {
    name.filter(|value| !value.trim().is_empty())
        .map(|container_name| {
            CreateContainerOptionsBuilder::new()
                .name(container_name)
                .build()
        })
}

pub(super) fn build_create_body(request: &CreateContainerRequest) -> ContainerCreateBody {
    ContainerCreateBody {
        image: Some(String::from(request.image())),
        cmd: request.cmd().map(<[String]>::to_vec),
        env: request.env().map(<[String]>::to_vec),
        labels: build_labels(request),
        working_dir: request.working_dir().map(String::from),
        host_config: Some(build_host_config(request)),
        ..ContainerCreateBody::default()
    }
}

/// Convert ordered labels into the engine's map form.
///
/// An empty label list yields `None` so the payload keeps its previous shape
/// for callers that do not use labels.
fn build_labels(request: &CreateContainerRequest) -> Option<HashMap<String, String>> {
    if request.labels().is_empty() {
        return None;
    }

    Some(
        request
            .labels()
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

fn build_host_config(request: &CreateContainerRequest) -> HostConfig {
    let mut host_config = security_host_config(request.security());

    if !request.mounts().is_empty() {
        host_config.mounts = Some(
            request
                .mounts()
                .iter()
                .map(|mount| Mount {
                    target: Some(String::from(mount.target().as_str())),
                    source: Some(String::from(mount.source().as_str())),
                    typ: Some(MountTypeEnum::BIND),
                    read_only: Some(mount.is_read_only()),
                    ..Mount::default()
                })
                .collect(),
        );
    }

    host_config
}

fn security_host_config(security: &ContainerSecurityOptions) -> HostConfig {
    if security.privileged {
        // In privileged mode, the engine host profile governs SELinux labelling
        // and device access; these minimal-mode toggles are intentionally
        // ignored.
        return HostConfig {
            privileged: Some(true),
            ..HostConfig::default()
        };
    }

    HostConfig {
        privileged: Some(false),
        cap_add: security
            .mount_dev_fuse
            .then(|| vec![String::from(CAP_SYS_ADMIN)]),
        devices: security.mount_dev_fuse.then(|| vec![fuse_device_mapping()]),
        security_opt: security
            .selinux_label_mode
            .requires_label_disable()
            .then(|| vec![String::from(SELINUX_LABEL_DISABLE)]),
        ..HostConfig::default()
    }
}

fn fuse_device_mapping() -> DeviceMapping {
    DeviceMapping {
        path_on_host: Some(String::from(DEV_FUSE_PATH)),
        path_in_container: Some(String::from(DEV_FUSE_PATH)),
        cgroup_permissions: Some(String::from(FUSE_DEVICE_PERMISSIONS)),
    }
}
