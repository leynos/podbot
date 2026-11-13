//! Container lifecycle operations behind an injectable client seam.
//!
//! Podbot start, stop, remove, inspect, and list operations all go through
//! [`ContainerLifecycleClient`]. Production code uses the `Bollard` `Docker`
//! implementation; tests inject a `mockall` double. The helpers on
//! [`EngineConnector`] translate engine errors into semantic
//! [`crate::error::ContainerError`] variants.

use std::future::Future;
use std::pin::Pin;

use bollard::Docker;
use bollard::errors::Error as BollardError;
use bollard::models::{ContainerInspectResponse, ContainerSummary};
use bollard::query_parameters::{ListContainersOptionsBuilder, StopContainerOptionsBuilder};

use super::EngineConnector;
use super::exec::block_on_runtime_handle as block_on_runtime;
use crate::error::{ContainerError, PodbotError};

const NOT_FOUND_STATUS_CODE: u16 = 404;
const NOT_MODIFIED_STATUS_CODE: u16 = 304;

/// Boxed future returned by [`ContainerLifecycleClient::start_container`].
pub type StartContainerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), BollardError>> + Send + 'a>>;

/// Boxed future returned by [`ContainerLifecycleClient::stop_container`].
pub type StopContainerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), BollardError>> + Send + 'a>>;

/// Boxed future returned by [`ContainerLifecycleClient::remove_container`].
pub type RemoveContainerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), BollardError>> + Send + 'a>>;

/// Boxed future returned by [`ContainerLifecycleClient::inspect_container`].
pub type InspectContainerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ContainerInspectResponse, BollardError>> + Send + 'a>>;

/// Boxed future returned by [`ContainerLifecycleClient::list_containers`].
pub type ListContainersFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ContainerSummary>, BollardError>> + Send + 'a>>;

/// Behaviour required to manage container lifecycle state.
///
/// The trait keeps orchestration testable without a live daemon.
pub trait ContainerLifecycleClient {
    /// Start a previously created container.
    fn start_container(&self, container_id: &str) -> StartContainerFuture<'_>;

    /// Stop a running container, waiting up to `grace_seconds` before killing it.
    fn stop_container(&self, container_id: &str, grace_seconds: i32) -> StopContainerFuture<'_>;

    /// Remove a container, optionally forcing removal of a running container.
    fn remove_container(&self, container_id: &str, force: bool) -> RemoveContainerFuture<'_>;

    /// Inspect a container for state and labels.
    fn inspect_container(&self, container_id: &str) -> InspectContainerFuture<'_>;

    /// List running containers matching a single `label=value` filter.
    fn list_containers(&self, label_filter: &str) -> ListContainersFuture<'_>;
}

impl ContainerLifecycleClient for Docker {
    fn start_container(&self, container_id: &str) -> StartContainerFuture<'_> {
        let owned_id = String::from(container_id);
        Box::pin(async move { Self::start_container(self, &owned_id, None).await })
    }

    fn stop_container(&self, container_id: &str, grace_seconds: i32) -> StopContainerFuture<'_> {
        let owned_id = String::from(container_id);
        let options = StopContainerOptionsBuilder::new().t(grace_seconds).build();
        Box::pin(async move { Self::stop_container(self, &owned_id, Some(options)).await })
    }

    fn remove_container(&self, container_id: &str, force: bool) -> RemoveContainerFuture<'_> {
        let owned_id = String::from(container_id);
        let options = bollard::query_parameters::RemoveContainerOptionsBuilder::new()
            .force(force)
            .build();
        Box::pin(async move { Self::remove_container(self, &owned_id, Some(options)).await })
    }

    fn inspect_container(&self, container_id: &str) -> InspectContainerFuture<'_> {
        let owned_id = String::from(container_id);
        Box::pin(async move { Self::inspect_container(self, &owned_id, None).await })
    }

    fn list_containers(&self, label_filter: &str) -> ListContainersFuture<'_> {
        let mut filters = std::collections::HashMap::new();
        filters.insert(String::from("label"), vec![String::from(label_filter)]);
        let options = ListContainersOptionsBuilder::new()
            .all(false)
            .filters(&filters)
            .build();
        Box::pin(async move { Self::list_containers(self, Some(options)).await })
    }
}

/// Outcome of a stop request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopOutcome {
    /// The container was running and is now stopped.
    Stopped,
    /// The container was already stopped when the request arrived.
    AlreadyStopped,
}

impl EngineConnector {
    /// Start a container using a caller-owned runtime handle.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::start_container_async`].
    pub fn start_container<C: ContainerLifecycleClient + Sync>(
        runtime: &tokio::runtime::Handle,
        client: &C,
        container_id: &str,
    ) -> Result<(), PodbotError> {
        block_on_runtime(runtime, async {
            Self::start_container_async(client, container_id).await
        })
    }

    /// Stop a container using a caller-owned runtime handle.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::stop_container_async`].
    pub fn stop_container<C: ContainerLifecycleClient + Sync>(
        runtime: &tokio::runtime::Handle,
        client: &C,
        container_id: &str,
        grace_seconds: i32,
    ) -> Result<StopOutcome, PodbotError> {
        block_on_runtime(runtime, async {
            Self::stop_container_async(client, container_id, grace_seconds).await
        })
    }

    /// Force-remove a container using a caller-owned runtime handle.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::remove_container_async`].
    pub fn remove_container<C: ContainerLifecycleClient + Sync>(
        runtime: &tokio::runtime::Handle,
        client: &C,
        container_id: &str,
    ) -> Result<(), PodbotError> {
        block_on_runtime(runtime, async {
            Self::remove_container_async(client, container_id).await
        })
    }

    /// Inspect a container using a caller-owned runtime handle.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::inspect_container_async`].
    pub fn inspect_container<C: ContainerLifecycleClient + Sync>(
        runtime: &tokio::runtime::Handle,
        client: &C,
        container_id: &str,
    ) -> Result<ContainerInspectResponse, PodbotError> {
        block_on_runtime(runtime, async {
            Self::inspect_container_async(client, container_id).await
        })
    }

    /// List managed containers using a caller-owned runtime handle.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::list_managed_containers_async`].
    pub fn list_managed_containers<C: ContainerLifecycleClient + Sync>(
        runtime: &tokio::runtime::Handle,
        client: &C,
        label_filter: &str,
    ) -> Result<Vec<ContainerSummary>, PodbotError> {
        block_on_runtime(runtime, async {
            Self::list_managed_containers_async(client, label_filter).await
        })
    }

    /// Start a container using an injected lifecycle client.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::StartFailed` when the engine rejects the start
    /// request.
    pub async fn start_container_async<C: ContainerLifecycleClient>(
        client: &C,
        container_id: &str,
    ) -> Result<(), PodbotError> {
        client.start_container(container_id).await.map_err(|error| {
            PodbotError::from(ContainerError::StartFailed {
                container_id: String::from(container_id),
                message: error.to_string(),
            })
        })
    }

    /// Stop a container using an injected lifecycle client.
    ///
    /// A `304 Not Modified` response means the container was already stopped
    /// and maps to [`StopOutcome::AlreadyStopped`] rather than an error.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::NotFound` for an unknown container and
    /// `ContainerError::StopFailed` for other engine failures.
    pub async fn stop_container_async<C: ContainerLifecycleClient>(
        client: &C,
        container_id: &str,
        grace_seconds: i32,
    ) -> Result<StopOutcome, PodbotError> {
        match client.stop_container(container_id, grace_seconds).await {
            Ok(()) => Ok(StopOutcome::Stopped),
            Err(error) if status_code(&error) == Some(NOT_MODIFIED_STATUS_CODE) => {
                Ok(StopOutcome::AlreadyStopped)
            }
            Err(error) if status_code(&error) == Some(NOT_FOUND_STATUS_CODE) => {
                Err(not_found(container_id))
            }
            Err(error) => Err(PodbotError::from(ContainerError::StopFailed {
                container_id: String::from(container_id),
                message: error.to_string(),
            })),
        }
    }

    /// Force-remove a container using an injected lifecycle client.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::NotFound` for an unknown container and
    /// `ContainerError::RemoveFailed` for other engine failures.
    pub async fn remove_container_async<C: ContainerLifecycleClient>(
        client: &C,
        container_id: &str,
    ) -> Result<(), PodbotError> {
        match client.remove_container(container_id, true).await {
            Ok(()) => Ok(()),
            Err(error) if status_code(&error) == Some(NOT_FOUND_STATUS_CODE) => {
                Err(not_found(container_id))
            }
            Err(error) => Err(PodbotError::from(ContainerError::RemoveFailed {
                container_id: String::from(container_id),
                message: error.to_string(),
            })),
        }
    }

    /// Inspect a container using an injected lifecycle client.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::NotFound` for an unknown container and
    /// `ContainerError::InspectFailed` for other engine failures.
    pub async fn inspect_container_async<C: ContainerLifecycleClient>(
        client: &C,
        container_id: &str,
    ) -> Result<ContainerInspectResponse, PodbotError> {
        match client.inspect_container(container_id).await {
            Ok(response) => Ok(response),
            Err(error) if status_code(&error) == Some(NOT_FOUND_STATUS_CODE) => {
                Err(not_found(container_id))
            }
            Err(error) => Err(PodbotError::from(ContainerError::InspectFailed {
                container_id: String::from(container_id),
                message: error.to_string(),
            })),
        }
    }

    /// List running containers matching a `label=value` filter.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::ListFailed` when the engine rejects the list
    /// request.
    pub async fn list_managed_containers_async<C: ContainerLifecycleClient>(
        client: &C,
        label_filter: &str,
    ) -> Result<Vec<ContainerSummary>, PodbotError> {
        client.list_containers(label_filter).await.map_err(|error| {
            PodbotError::from(ContainerError::ListFailed {
                message: error.to_string(),
            })
        })
    }
}

fn not_found(container_id: &str) -> PodbotError {
    PodbotError::from(ContainerError::NotFound {
        container: String::from(container_id),
    })
}

/// Extract the HTTP status code from a Docker response error.
const fn status_code(error: &BollardError) -> Option<u16> {
    match error {
        BollardError::DockerResponseServerError { status_code, .. } => Some(*status_code),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
