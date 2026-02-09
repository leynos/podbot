//! Socket resolution and container engine connection.
//!
//! This module provides functionality to resolve container engine socket endpoints
//! from multiple sources (environment variables, configuration, platform defaults)
//! and establish connections using the `Bollard` library.

mod error_classification;
mod health_check;

use std::fmt;

use bollard::Docker;
use error_classification::classify_connection_error;

use crate::error::PodbotError;

// =============================================================================
// SocketPath newtype
// =============================================================================

/// A validated container engine socket endpoint path.
///
/// This newtype wraps a socket endpoint string, providing type safety and
/// reducing the number of raw string arguments in function signatures.
/// Socket paths can be Unix sockets (`unix:///path`), Windows named pipes
/// (`npipe:////./pipe/name`), or HTTP endpoints (`http://host:port`).
///
/// # Examples
///
/// ```ignore
/// use podbot::engine::SocketPath;
///
/// let socket = SocketPath::new("unix:///var/run/docker.sock");
/// assert_eq!(socket.as_str(), "unix:///var/run/docker.sock");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketPath(String);

impl SocketPath {
    /// Creates a new `SocketPath` from any string-like type.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Returns the socket path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the `SocketPath` and returns the inner `String`.
    #[expect(
        dead_code,
        reason = "public API for completeness; callers may need owned String"
    )]
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl From<String> for SocketPath {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for SocketPath {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl AsRef<str> for SocketPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for SocketPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// =============================================================================
// Constants
// =============================================================================

/// Environment variable names checked in fallback order after configuration sources.
const FALLBACK_ENV_VARS: &[&str] = &["DOCKER_HOST", "CONTAINER_HOST", "PODMAN_HOST"];

/// Connection timeout in seconds for Docker/Podman API connections.
const CONNECTION_TIMEOUT_SECS: u64 = 120;

/// Timeout in seconds for health check operations.
pub(super) const HEALTH_CHECK_TIMEOUT_SECS: u64 = 10;

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
    fn is_socket_scheme(socket: impl AsRef<str>) -> bool {
        let s = socket.as_ref();
        s.starts_with("unix://") || s.starts_with("npipe://")
    }

    /// Returns true if the socket string has an HTTP-compatible scheme.
    fn is_http_scheme(socket: impl AsRef<str>) -> bool {
        let s = socket.as_ref();
        s.starts_with("tcp://") || s.starts_with("http://") || s.starts_with("https://")
    }

    /// Classify a socket string by its scheme prefix.
    fn classify(socket: impl AsRef<str>) -> Self {
        let s = socket.as_ref();
        match (Self::is_socket_scheme(s), Self::is_http_scheme(s)) {
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
    /// Returns a `ContainerError` variant:
    /// - `ContainerError::SocketNotFound` if the socket file does not exist.
    /// - `ContainerError::PermissionDenied` if the user lacks socket access.
    /// - `ContainerError::ConnectionFailed` for all other connection failures.
    pub fn connect(socket: impl AsRef<str>) -> Result<Docker, PodbotError> {
        let socket_str = socket.as_ref();
        let (docker_result, socket_for_error) = match SocketType::classify(socket_str) {
            SocketType::Socket => (
                Docker::connect_with_socket(
                    socket_str,
                    CONNECTION_TIMEOUT_SECS,
                    bollard::API_DEFAULT_VERSION,
                ),
                socket_str.to_owned(),
            ),
            SocketType::Http => {
                // Rewrite tcp:// to http:// for Bollard compatibility
                let http_socket = if socket_str.starts_with("tcp://") {
                    socket_str.replacen("tcp://", "http://", 1)
                } else {
                    socket_str.to_owned()
                };
                (
                    Docker::connect_with_http(
                        &http_socket,
                        CONNECTION_TIMEOUT_SECS,
                        bollard::API_DEFAULT_VERSION,
                    ),
                    http_socket,
                )
            }
            SocketType::BarePath => {
                let socket_uri = Self::normalize_bare_path(socket_str);
                (
                    Docker::connect_with_socket(
                        &socket_uri,
                        CONNECTION_TIMEOUT_SECS,
                        bollard::API_DEFAULT_VERSION,
                    ),
                    socket_uri,
                )
            }
        };
        let docker = docker_result
            .map_err(|e| PodbotError::from(classify_connection_error(&e, &socket_for_error)))?;

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
    fn normalize_bare_path(path: impl AsRef<str>) -> String {
        let p = path.as_ref();
        // Named pipes typically start with \\ or // (e.g., \\.\pipe\docker_engine)
        if p.starts_with("\\\\") || p.starts_with("//") {
            format!("npipe://{p}")
        } else {
            format!("unix://{p}")
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
    /// Returns a `ContainerError` variant (`ContainerError::SocketNotFound`,
    /// `ContainerError::PermissionDenied`, or
    /// `ContainerError::ConnectionFailed`) if the connection cannot be
    /// established.
    pub fn connect_with_fallback<S: AsRef<str> + ?Sized, E: mockable::Env>(
        config_socket: Option<&S>,
        resolver: &SocketResolver<'_, E>,
    ) -> Result<Docker, PodbotError> {
        let cfg_socket = config_socket.map(AsRef::as_ref);
        let socket = Self::resolve_socket(cfg_socket, resolver);
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
    pub fn resolve_socket<S: AsRef<str> + ?Sized, E: mockable::Env>(
        config_socket: Option<&S>,
        resolver: &SocketResolver<'_, E>,
    ) -> String {
        let cfg_socket = config_socket.map(AsRef::as_ref);
        cfg_socket
            .filter(|s| !s.is_empty())
            .map(String::from)
            .or_else(|| resolver.resolve_from_env())
            .unwrap_or_else(|| SocketResolver::<E>::default_socket().to_owned())
    }
}

#[cfg(test)]
mod tests;
