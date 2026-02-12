//! Container creation with configurable sandbox security options.
//!
//! This module translates high-level security settings into `Bollard`
//! container-create payloads and provides async/sync helpers for creating
//! containers.

use std::future::Future;
use std::pin::Pin;

use bollard::Docker;
use bollard::models::{ContainerCreateBody, ContainerCreateResponse, DeviceMapping, HostConfig};
use bollard::query_parameters::{CreateContainerOptions, CreateContainerOptionsBuilder};

use super::EngineConnector;
use crate::config::SandboxConfig;
use crate::error::{ConfigError, ContainerError, PodbotError};

const DEV_FUSE_PATH: &str = "/dev/fuse";
const FUSE_DEVICE_PERMISSIONS: &str = "rwm";
const CAP_SYS_ADMIN: &str = "SYS_ADMIN";
const SELINUX_LABEL_DISABLE: &str = "label=disable";

/// Boxed future type returned by [`ContainerCreator`] implementors.
pub type CreateContainerFuture<'a> = Pin<
    Box<dyn Future<Output = Result<ContainerCreateResponse, bollard::errors::Error>> + Send + 'a>,
>;

/// Behaviour required to create a container via a backing engine client.
///
/// This abstraction exists to keep container-creation logic testable without a
/// running daemon.
pub trait ContainerCreator {
    /// Create a container from `Bollard` options and body payload.
    fn create_container(
        &self,
        options: Option<CreateContainerOptions>,
        config: ContainerCreateBody,
    ) -> CreateContainerFuture<'_>;
}

impl ContainerCreator for Docker {
    fn create_container(
        &self,
        options: Option<CreateContainerOptions>,
        config: ContainerCreateBody,
    ) -> CreateContainerFuture<'_> {
        Box::pin(async move { Self::create_container(self, options, config).await })
    }
}

/// How `SELinux` labels should be applied to the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelinuxLabelMode {
    /// Keep engine defaults for `SELinux` labels.
    KeepDefault,

    /// Disable labels for the container process.
    #[default]
    DisableForContainer,
}

/// Container security options applied at create time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerSecurityOptions {
    /// Whether the container should run in privileged mode.
    pub privileged: bool,

    /// Whether `/dev/fuse` should be mounted into the container.
    pub mount_dev_fuse: bool,

    /// `SELinux` label handling mode.
    pub selinux_label_mode: SelinuxLabelMode,
}

impl ContainerSecurityOptions {
    /// Build security options from `[sandbox]` configuration.
    #[must_use]
    pub const fn from_sandbox_config(sandbox: &SandboxConfig) -> Self {
        let selinux_label_mode = if sandbox.privileged {
            SelinuxLabelMode::KeepDefault
        } else {
            SelinuxLabelMode::DisableForContainer
        };

        Self {
            privileged: sandbox.privileged,
            mount_dev_fuse: sandbox.mount_dev_fuse,
            selinux_label_mode,
        }
    }
}

impl Default for ContainerSecurityOptions {
    fn default() -> Self {
        Self {
            privileged: false,
            mount_dev_fuse: true,
            selinux_label_mode: SelinuxLabelMode::DisableForContainer,
        }
    }
}

/// Container-creation request parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateContainerRequest {
    /// The container image to create from.
    image: String,

    /// Optional container name.
    name: Option<String>,

    /// Optional command to run in the container.
    cmd: Option<Vec<String>>,

    /// Optional environment variables in `KEY=value` form.
    env: Option<Vec<String>>,

    /// Security profile to apply.
    security: ContainerSecurityOptions,
}

impl CreateContainerRequest {
    /// Create a request with image and security settings.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` when `image` is empty or
    /// whitespace-only.
    pub fn new(
        image: impl Into<String>,
        security: ContainerSecurityOptions,
    ) -> Result<Self, PodbotError> {
        let image_value = image.into();
        let validated_image = String::from(validate_image(&image_value)?);

        Ok(Self {
            image: validated_image,
            name: None,
            cmd: None,
            env: None,
            security,
        })
    }

    /// Attach an optional container name.
    #[must_use]
    pub fn with_name(mut self, name: Option<String>) -> Self {
        self.name = name.filter(|value| !value.trim().is_empty());
        self
    }

    /// Attach an optional command vector.
    #[must_use]
    pub fn with_cmd(mut self, cmd: Option<Vec<String>>) -> Self {
        self.cmd = cmd;
        self
    }

    /// Attach optional environment entries.
    #[must_use]
    pub fn with_env(mut self, env: Option<Vec<String>>) -> Self {
        self.env = env;
        self
    }

    /// Return the configured image.
    #[must_use]
    pub fn image(&self) -> &str {
        &self.image
    }

    /// Return the optional configured name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Return the optional configured command.
    #[must_use]
    pub fn cmd(&self) -> Option<&[String]> {
        self.cmd.as_deref()
    }

    /// Return the optional configured environment list.
    #[must_use]
    pub fn env(&self) -> Option<&[String]> {
        self.env.as_deref()
    }

    /// Return the configured security options.
    #[must_use]
    pub const fn security(&self) -> &ContainerSecurityOptions {
        &self.security
    }
}

impl EngineConnector {
    /// Create a container using a provided client abstraction (async version).
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::CreateFailed` when the engine rejects the
    /// create request.
    pub async fn create_container_async<C: ContainerCreator>(
        creator: &C,
        request: &CreateContainerRequest,
    ) -> Result<String, PodbotError> {
        let options = build_create_options(request.name());
        let config = build_create_body(request);

        let response = creator
            .create_container(options, config)
            .await
            .map_err(|error| {
                PodbotError::from(ContainerError::CreateFailed {
                    message: error.to_string(),
                })
            })?;

        Ok(response.id)
    }

    /// Create a container using a provided client abstraction.
    ///
    /// This synchronous helper blocks on [`Self::create_container_async`] using
    /// an existing Tokio runtime handle supplied by the caller.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::CreateFailed` when the engine rejects the
    /// create request.
    pub fn create_container<C: ContainerCreator>(
        runtime: &tokio::runtime::Handle,
        creator: &C,
        request: &CreateContainerRequest,
    ) -> Result<String, PodbotError> {
        runtime.block_on(Self::create_container_async(creator, request))
    }
}

fn validate_image(image: &str) -> Result<&str, PodbotError> {
    let trimmed = image.trim();

    if trimmed.is_empty() {
        return Err(PodbotError::from(ConfigError::MissingRequired {
            field: String::from("image"),
        }));
    }

    Ok(trimmed)
}

fn build_create_options(name: Option<&str>) -> Option<CreateContainerOptions> {
    name.filter(|value| !value.trim().is_empty())
        .map(|container_name| {
            CreateContainerOptionsBuilder::new()
                .name(container_name)
                .build()
        })
}

fn build_create_body(request: &CreateContainerRequest) -> ContainerCreateBody {
    ContainerCreateBody {
        image: Some(String::from(request.image())),
        cmd: request.cmd().map(<[String]>::to_vec),
        env: request.env().map(<[String]>::to_vec),
        host_config: Some(build_host_config(request.security())),
        ..ContainerCreateBody::default()
    }
}

fn build_host_config(security: &ContainerSecurityOptions) -> HostConfig {
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

impl SelinuxLabelMode {
    const fn requires_label_disable(self) -> bool {
        matches!(self, Self::DisableForContainer)
    }
}

#[cfg(test)]
mod tests;
