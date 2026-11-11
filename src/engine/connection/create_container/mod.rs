//! Container creation with configurable sandbox security options.
//!
//! This module translates high-level security settings into `Bollard`
//! container-create payloads and provides async/sync helpers for creating
//! containers. Requests may also carry ownership labels, bind mounts, and a
//! working directory so orchestration can describe the full sandbox.

mod body;
mod request;

use std::future::Future;
use std::pin::Pin;

use bollard::Docker;
use bollard::models::{ContainerCreateBody, ContainerCreateResponse};
use bollard::query_parameters::CreateContainerOptions;

use self::body::{build_create_body, build_create_options};
use super::EngineConnector;
use crate::config::SandboxConfig;
pub use crate::config::SelinuxLabelMode;
use crate::error::{ContainerError, PodbotError};
pub use request::{BindMount, CreateContainerRequest};

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
        Self {
            privileged: sandbox.privileged,
            mount_dev_fuse: sandbox.mount_dev_fuse,
            selinux_label_mode: sandbox.selinux_label_mode,
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

impl SelinuxLabelMode {
    const fn requires_label_disable(self) -> bool {
        matches!(self, Self::DisableForContainer)
    }
}

#[cfg(test)]
mod tests;
