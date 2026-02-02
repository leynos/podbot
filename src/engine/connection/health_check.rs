//! Health check and connect-and-verify functionality.
//!
//! This module provides health check operations for container engines,
//! including async and sync variants for both standalone health checks
//! and combined connect-and-verify operations.

use std::time::Duration;

use bollard::Docker;

use super::{EngineConnector, HEALTH_CHECK_TIMEOUT_SECS, SocketResolver};
use crate::error::{ContainerError, PodbotError};

impl EngineConnector {
    // =========================================================================
    // Health check - internal helper
    // =========================================================================

    /// Perform a ping with timeout (internal helper).
    ///
    /// This is the core async implementation reused by all health check APIs.
    pub(super) async fn ping_with_timeout(docker: &Docker) -> Result<(), PodbotError> {
        let timeout = Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS);

        tokio::time::timeout(timeout, docker.ping())
            .await
            .map_err(|_| {
                PodbotError::from(ContainerError::HealthCheckTimeout {
                    seconds: HEALTH_CHECK_TIMEOUT_SECS,
                })
            })?
            .map_err(|e| {
                PodbotError::from(ContainerError::HealthCheckFailed {
                    message: e.to_string(),
                })
            })?;
        Ok(())
    }

    // =========================================================================
    // Health check - public APIs
    // =========================================================================

    /// Verify the container engine is responsive (async version).
    ///
    /// Sends a ping request to the engine and waits for a response.
    /// This confirms the engine is operational, not just that the socket
    /// is reachable.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::HealthCheckFailed` if the engine does not
    /// respond correctly.
    ///
    /// Returns `ContainerError::HealthCheckTimeout` if the check times out.
    pub async fn health_check_async(docker: &Docker) -> Result<(), PodbotError> {
        Self::ping_with_timeout(docker).await
    }

    /// Verify the container engine is responsive.
    ///
    /// Sends a ping request to the engine and waits for a response.
    /// This confirms the engine is operational, not just that the socket
    /// is reachable.
    ///
    /// This is the synchronous version that creates a dedicated tokio runtime
    /// to execute the async health check. Use [`Self::health_check_async`]
    /// when already in an async context to avoid the runtime creation overhead.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::RuntimeCreationFailed` if the tokio runtime
    /// cannot be created.
    ///
    /// Returns `ContainerError::HealthCheckFailed` if the engine does not
    /// respond correctly.
    ///
    /// Returns `ContainerError::HealthCheckTimeout` if the check times out.
    pub fn health_check(docker: &Docker) -> Result<(), PodbotError> {
        let rt = Self::create_runtime()?;
        rt.block_on(Self::health_check_async(docker))
    }

    /// Create a tokio runtime for synchronous operations.
    pub(super) fn create_runtime() -> Result<tokio::runtime::Runtime, PodbotError> {
        tokio::runtime::Runtime::new().map_err(|e| {
            PodbotError::from(ContainerError::RuntimeCreationFailed {
                message: e.to_string(),
            })
        })
    }

    // =========================================================================
    // Connect and verify - internal helper
    // =========================================================================

    /// Connect then verify (internal helper).
    ///
    /// This is the core async combinator reused by all connect-and-verify APIs.
    pub(super) async fn connect_then_verify<F>(connect_fn: F) -> Result<Docker, PodbotError>
    where
        F: FnOnce() -> Result<Docker, PodbotError>,
    {
        let docker = connect_fn()?;
        Self::ping_with_timeout(&docker).await?;
        Ok(docker)
    }

    // =========================================================================
    // Connect and verify - public APIs
    // =========================================================================

    /// Connect to the container engine and verify it responds (async version).
    ///
    /// Combines `connect()` with `health_check_async()` in a single operation.
    /// Useful when the caller wants to ensure the engine is fully operational
    /// before proceeding.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::ConnectionFailed` if the connection fails.
    ///
    /// Returns `ContainerError::HealthCheckFailed` if the health check fails.
    ///
    /// Returns `ContainerError::HealthCheckTimeout` if the check times out.
    pub async fn connect_and_verify_async(socket: impl AsRef<str>) -> Result<Docker, PodbotError> {
        let socket_str = socket.as_ref();
        Self::connect_then_verify(|| Self::connect(socket_str)).await
    }

    /// Connect to the container engine and verify it responds.
    ///
    /// Combines `connect()` with `health_check()` in a single operation.
    /// Useful when the caller wants to ensure the engine is fully operational
    /// before proceeding.
    ///
    /// This is the synchronous version that creates a dedicated tokio runtime.
    /// Use [`Self::connect_and_verify_async`] when already in an async context.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::RuntimeCreationFailed` if the tokio runtime
    /// cannot be created.
    ///
    /// Returns `ContainerError::ConnectionFailed` if the connection fails.
    ///
    /// Returns `ContainerError::HealthCheckFailed` if the health check fails.
    ///
    /// Returns `ContainerError::HealthCheckTimeout` if the check times out.
    pub fn connect_and_verify(socket: impl AsRef<str>) -> Result<Docker, PodbotError> {
        let socket_str = socket.as_ref();
        let rt = Self::create_runtime()?;
        rt.block_on(Self::connect_and_verify_async(socket_str))
    }

    /// Connect using fallback resolution and verify the engine responds
    /// (async version).
    ///
    /// Combines `connect_with_fallback()` with `health_check_async()`.
    ///
    /// Resolution order:
    /// 1. `config_socket` (from CLI, config file, or `PODBOT_ENGINE_SOCKET`)
    /// 2. `DOCKER_HOST`, `CONTAINER_HOST`, `PODMAN_HOST` (via resolver)
    /// 3. Platform default socket
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::ConnectionFailed` if the connection fails.
    ///
    /// Returns `ContainerError::HealthCheckFailed` if the health check fails.
    ///
    /// Returns `ContainerError::HealthCheckTimeout` if the check times out.
    pub async fn connect_with_fallback_and_verify_async<
        S: AsRef<str> + ?Sized,
        E: mockable::Env,
    >(
        config_socket: Option<&S>,
        resolver: &SocketResolver<'_, E>,
    ) -> Result<Docker, PodbotError> {
        let cfg_socket = config_socket.map(AsRef::as_ref);
        Self::connect_then_verify(|| Self::connect_with_fallback(cfg_socket, resolver)).await
    }

    /// Connect using fallback resolution and verify the engine responds.
    ///
    /// Combines `connect_with_fallback()` with `health_check()`.
    ///
    /// Resolution order:
    /// 1. `config_socket` (from CLI, config file, or `PODBOT_ENGINE_SOCKET`)
    /// 2. `DOCKER_HOST`, `CONTAINER_HOST`, `PODMAN_HOST` (via resolver)
    /// 3. Platform default socket
    ///
    /// This is the synchronous version that creates a dedicated tokio runtime.
    /// Use [`Self::connect_with_fallback_and_verify_async`] when already in an
    /// async context.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::RuntimeCreationFailed` if the tokio runtime
    /// cannot be created.
    ///
    /// Returns `ContainerError::ConnectionFailed` if the connection fails.
    ///
    /// Returns `ContainerError::HealthCheckFailed` if the health check fails.
    ///
    /// Returns `ContainerError::HealthCheckTimeout` if the check times out.
    pub fn connect_with_fallback_and_verify<S: AsRef<str> + ?Sized, E: mockable::Env>(
        config_socket: Option<&S>,
        resolver: &SocketResolver<'_, E>,
    ) -> Result<Docker, PodbotError> {
        let cfg_socket = config_socket.map(AsRef::as_ref);
        let rt = Self::create_runtime()?;
        rt.block_on(Self::connect_with_fallback_and_verify_async(
            cfg_socket, resolver,
        ))
    }
}
