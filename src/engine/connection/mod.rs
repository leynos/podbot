//! Socket resolution and container engine connection.
//!
//! This module provides functionality to resolve container engine socket endpoints
//! from multiple sources (environment variables, configuration, platform defaults)
//! and establish connections using the Bollard library.

use std::time::Duration;

use bollard::Docker;

use crate::error::{ContainerError, PodbotError};

/// Environment variable names checked in fallback order after configuration sources.
const FALLBACK_ENV_VARS: &[&str] = &["DOCKER_HOST", "CONTAINER_HOST", "PODMAN_HOST"];

/// Connection timeout in seconds for Docker/Podman API connections.
const CONNECTION_TIMEOUT_SECS: u64 = 120;

/// Timeout in seconds for health check operations.
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 10;

/// Default socket path for Unix platforms.
#[cfg(unix)]
const DEFAULT_SOCKET: &str = "unix:///var/run/docker.sock";

/// Default socket path for Windows platforms.
#[cfg(windows)]
const DEFAULT_SOCKET: &str = "npipe:////./pipe/docker_engine";

/// Resolves container engine socket endpoints from environment variables.
///
/// The resolver checks a prioritized list of environment variables to find
/// the socket endpoint when no explicit configuration is provided.
///
/// # Type Parameters
///
/// * `E` - An environment provider implementing the `mockable::Env` trait,
///   allowing for testable environment variable access.
///
/// # Example
///
/// ```ignore
/// use mockable::DefaultEnv;
/// use podbot::engine::SocketResolver;
///
/// let env = DefaultEnv::new();
/// let resolver = SocketResolver::new(&env);
///
/// if let Some(socket) = resolver.resolve_from_env() {
///     println!("Found socket: {}", socket);
/// }
/// ```
pub struct SocketResolver<'a, E: mockable::Env> {
    env: &'a E,
}

impl<'a, E: mockable::Env> SocketResolver<'a, E> {
    /// Creates a new socket resolver with the given environment provider.
    #[must_use]
    pub const fn new(env: &'a E) -> Self {
        Self { env }
    }

    /// Resolves the socket endpoint from fallback environment variables.
    ///
    /// Checks the following environment variables in order:
    /// 1. `DOCKER_HOST`
    /// 2. `CONTAINER_HOST`
    /// 3. `PODMAN_HOST`
    ///
    /// Returns `None` if no fallback variable is set or all are empty.
    #[must_use]
    pub fn resolve_from_env(&self) -> Option<String> {
        FALLBACK_ENV_VARS
            .iter()
            .filter_map(|var_name| self.env.string(var_name))
            .find(|value| !value.is_empty())
    }

    /// Returns the platform default socket path.
    ///
    /// On Unix systems, this is `unix:///var/run/docker.sock`.
    /// On Windows systems, this is `npipe:////./pipe/docker_engine`.
    #[must_use]
    pub const fn default_socket() -> &'static str {
        DEFAULT_SOCKET
    }
}

/// Classifies socket endpoint types for connection handling.
enum SocketType {
    /// Unix socket or Windows named pipe with explicit scheme.
    Socket,
    /// HTTP, HTTPS, or TCP endpoint (TCP is rewritten to HTTP).
    Http,
    /// Bare path without scheme prefix.
    BarePath,
}

impl SocketType {
    /// Returns true if the socket string has a Unix or named pipe scheme.
    fn is_socket_scheme(socket: &str) -> bool {
        socket.starts_with("unix://") || socket.starts_with("npipe://")
    }

    /// Returns true if the socket string has an HTTP-compatible scheme.
    fn is_http_scheme(socket: &str) -> bool {
        socket.starts_with("tcp://")
            || socket.starts_with("http://")
            || socket.starts_with("https://")
    }

    /// Classify a socket string by its scheme prefix.
    fn classify(socket: &str) -> Self {
        match (Self::is_socket_scheme(socket), Self::is_http_scheme(socket)) {
            (true, _) => Self::Socket,
            (_, true) => Self::Http,
            _ => Self::BarePath,
        }
    }
}

/// Provides methods to connect to Docker or Podman container engines.
///
/// The connector supports Unix sockets, Windows named pipes, HTTP, and HTTPS
/// endpoints.
pub struct EngineConnector;

impl EngineConnector {
    /// Connect to the container engine at the specified socket path.
    ///
    /// Supports the following endpoint formats:
    /// - Unix sockets: `unix:///path/to/socket`
    /// - Windows named pipes: `npipe:////./pipe/name`
    /// - TCP: `tcp://host:port` (treated as HTTP connection)
    /// - HTTP: `http://host:port`
    /// - HTTPS: `https://host:port`
    /// - Bare paths: Paths starting with `\\` or `//` are treated as Windows
    ///   named pipes (e.g., `//./pipe/docker_engine`). All other paths are
    ///   treated as Unix sockets (e.g., `/var/run/docker.sock`). Detection is
    ///   syntax-based, not platform-based.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::ConnectionFailed` if the connection cannot be
    /// established.
    pub fn connect(socket: &str) -> Result<Docker, PodbotError> {
        let docker = match SocketType::classify(socket) {
            SocketType::Socket => Docker::connect_with_socket(
                socket,
                CONNECTION_TIMEOUT_SECS,
                bollard::API_DEFAULT_VERSION,
            ),
            SocketType::Http => {
                // Rewrite tcp:// to http:// for Bollard compatibility
                let http_socket = if socket.starts_with("tcp://") {
                    socket.replacen("tcp://", "http://", 1)
                } else {
                    socket.to_owned()
                };
                Docker::connect_with_http(
                    &http_socket,
                    CONNECTION_TIMEOUT_SECS,
                    bollard::API_DEFAULT_VERSION,
                )
            }
            SocketType::BarePath => {
                let socket_uri = Self::normalize_bare_path(socket);
                Docker::connect_with_socket(
                    &socket_uri,
                    CONNECTION_TIMEOUT_SECS,
                    bollard::API_DEFAULT_VERSION,
                )
            }
        }
        .map_err(|e| {
            PodbotError::from(ContainerError::ConnectionFailed {
                message: e.to_string(),
            })
        })?;

        Ok(docker)
    }

    /// Normalize a bare socket path to a URI with the appropriate scheme.
    ///
    /// Paths starting with `\\` or `//` are assumed to be Windows named pipe
    /// paths (e.g., `\\.\pipe\docker_engine`) and are prefixed with `npipe://`.
    /// All other paths are assumed to be Unix socket paths and are prefixed
    /// with `unix://`.
    ///
    /// Note: This detection is based on path syntax, not the current platform.
    /// Paths like `//some/path` will be treated as named pipes even on Unix.
    fn normalize_bare_path(path: &str) -> String {
        // Named pipes typically start with \\ or // (e.g., \\.\pipe\docker_engine)
        if path.starts_with("\\\\") || path.starts_with("//") {
            format!("npipe://{path}")
        } else {
            format!("unix://{path}")
        }
    }

    /// Connect using the resolved socket from configuration and environment.
    ///
    /// Resolution order:
    /// 1. `config_socket` (from CLI, config file, or `PODBOT_ENGINE_SOCKET`)
    /// 2. `DOCKER_HOST`, `CONTAINER_HOST`, `PODMAN_HOST` (via resolver)
    /// 3. Platform default socket
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::ConnectionFailed` if the connection cannot be
    /// established.
    pub fn connect_with_fallback<E: mockable::Env>(
        config_socket: Option<&str>,
        resolver: &SocketResolver<'_, E>,
    ) -> Result<Docker, PodbotError> {
        let socket = Self::resolve_socket(config_socket, resolver);
        Self::connect(&socket)
    }

    /// Resolves the socket endpoint without establishing a connection.
    ///
    /// This is useful for logging or displaying the resolved socket path
    /// before attempting a connection.
    ///
    /// Resolution order:
    /// 1. `config_socket` (from CLI, config file, or `PODBOT_ENGINE_SOCKET`)
    /// 2. `DOCKER_HOST`, `CONTAINER_HOST`, `PODMAN_HOST` (via resolver)
    /// 3. Platform default socket
    #[must_use]
    pub fn resolve_socket<E: mockable::Env>(
        config_socket: Option<&str>,
        resolver: &SocketResolver<'_, E>,
    ) -> String {
        config_socket
            .filter(|s| !s.is_empty())
            .map(String::from)
            .or_else(|| resolver.resolve_from_env())
            .unwrap_or_else(|| SocketResolver::<E>::default_socket().to_owned())
    }

    // =========================================================================
    // Health check - internal helper
    // =========================================================================

    /// Perform a ping with timeout (internal helper).
    ///
    /// This is the core async implementation reused by all health check APIs.
    async fn ping_with_timeout(docker: &Docker) -> Result<(), PodbotError> {
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
    fn create_runtime() -> Result<tokio::runtime::Runtime, PodbotError> {
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
    async fn connect_then_verify<F>(connect_fn: F) -> Result<Docker, PodbotError>
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
    pub async fn connect_and_verify_async(socket: &str) -> Result<Docker, PodbotError> {
        Self::connect_then_verify(|| Self::connect(socket)).await
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
    pub fn connect_and_verify(socket: &str) -> Result<Docker, PodbotError> {
        let rt = Self::create_runtime()?;
        rt.block_on(Self::connect_and_verify_async(socket))
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
    pub async fn connect_with_fallback_and_verify_async<E: mockable::Env>(
        config_socket: Option<&str>,
        resolver: &SocketResolver<'_, E>,
    ) -> Result<Docker, PodbotError> {
        Self::connect_then_verify(|| Self::connect_with_fallback(config_socket, resolver)).await
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
    pub fn connect_with_fallback_and_verify<E: mockable::Env>(
        config_socket: Option<&str>,
        resolver: &SocketResolver<'_, E>,
    ) -> Result<Docker, PodbotError> {
        let rt = Self::create_runtime()?;
        rt.block_on(Self::connect_with_fallback_and_verify_async(
            config_socket,
            resolver,
        ))
    }
}

#[cfg(test)]
mod tests;
