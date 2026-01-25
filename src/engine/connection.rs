//! Socket resolution and container engine connection.
//!
//! This module provides functionality to resolve container engine socket endpoints
//! from multiple sources (environment variables, configuration, platform defaults)
//! and establish connections using the Bollard library.

use bollard::Docker;

use crate::error::{ContainerError, PodbotError};

/// Environment variable names checked in fallback order after configuration sources.
const FALLBACK_ENV_VARS: &[&str] = &["DOCKER_HOST", "CONTAINER_HOST", "PODMAN_HOST"];

/// Default socket path for Unix platforms.
#[cfg(unix)]
const DEFAULT_SOCKET: &str = "unix:///var/run/docker.sock";

/// Default socket path for Windows platforms.
#[cfg(windows)]
const DEFAULT_SOCKET: &str = "npipe:////./pipe/docker_engine";

/// Resolves container engine socket endpoints from environment variables.
///
/// The resolver checks a prioritised list of environment variables to find
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
    /// - HTTP: `http://host:port`
    /// - HTTPS: `https://host:port`
    /// - Bare paths are treated as Unix sockets: `/var/run/docker.sock`
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::ConnectionFailed` if the connection cannot be
    /// established.
    pub fn connect(socket: &str) -> Result<Docker, PodbotError> {
        let docker = if socket.starts_with("unix://") || socket.starts_with("npipe://") {
            Docker::connect_with_socket(socket, 120, bollard::API_DEFAULT_VERSION)
        } else if socket.starts_with("http://") || socket.starts_with("https://") {
            Docker::connect_with_http(socket, 120, bollard::API_DEFAULT_VERSION)
        } else {
            // Treat bare paths as Unix sockets
            let socket_uri = format!("unix://{socket}");
            Docker::connect_with_socket(&socket_uri, 120, bollard::API_DEFAULT_VERSION)
        }
        .map_err(|e| {
            PodbotError::from(ContainerError::ConnectionFailed {
                message: e.to_string(),
            })
        })?;

        Ok(docker)
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
        let socket = config_socket
            .map(String::from)
            .or_else(|| resolver.resolve_from_env())
            .unwrap_or_else(|| SocketResolver::<E>::default_socket().to_owned());

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
            .map(String::from)
            .or_else(|| resolver.resolve_from_env())
            .unwrap_or_else(|| SocketResolver::<E>::default_socket().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockable::MockEnv;
    use rstest::rstest;

    /// Creates a `MockEnv` that returns `None` for all environment variable queries.
    fn empty_env() -> MockEnv {
        let mut env = MockEnv::new();
        env.expect_string().returning(|_| None);
        env
    }

    /// Creates a `MockEnv` that returns the specified value for `DOCKER_HOST`.
    fn env_with_docker_host(value: &'static str) -> MockEnv {
        let mut env = MockEnv::new();
        env.expect_string().returning(move |key| {
            if key == "DOCKER_HOST" {
                Some(String::from(value))
            } else {
                None
            }
        });
        env
    }

    /// Creates a `MockEnv` with custom mappings for environment variables.
    fn env_with_vars(mappings: &'static [(&'static str, &'static str)]) -> MockEnv {
        let mut env = MockEnv::new();
        env.expect_string().returning(move |key| {
            mappings
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| String::from(*v))
        });
        env
    }

    #[rstest]
    fn resolver_returns_none_when_no_env_vars_set() {
        let env = empty_env();
        let resolver = SocketResolver::new(&env);
        assert!(resolver.resolve_from_env().is_none());
    }

    #[rstest]
    fn resolver_returns_docker_host_when_set() {
        let env = env_with_docker_host("unix:///custom/docker.sock");
        let resolver = SocketResolver::new(&env);
        assert_eq!(
            resolver.resolve_from_env(),
            Some(String::from("unix:///custom/docker.sock"))
        );
    }

    #[rstest]
    fn resolver_respects_docker_host() {
        let env = env_with_vars(&[("DOCKER_HOST", "unix:///docker.sock")]);
        let resolver = SocketResolver::new(&env);
        assert_eq!(
            resolver.resolve_from_env(),
            Some(String::from("unix:///docker.sock"))
        );
    }

    #[rstest]
    fn resolver_respects_container_host() {
        let env = env_with_vars(&[("CONTAINER_HOST", "unix:///container.sock")]);
        let resolver = SocketResolver::new(&env);
        assert_eq!(
            resolver.resolve_from_env(),
            Some(String::from("unix:///container.sock"))
        );
    }

    #[rstest]
    fn resolver_respects_podman_host() {
        let env = env_with_vars(&[("PODMAN_HOST", "unix:///podman.sock")]);
        let resolver = SocketResolver::new(&env);
        assert_eq!(
            resolver.resolve_from_env(),
            Some(String::from("unix:///podman.sock"))
        );
    }

    #[rstest]
    fn resolver_prefers_docker_host_over_podman_host() {
        let env = env_with_vars(&[
            ("DOCKER_HOST", "unix:///docker.sock"),
            ("PODMAN_HOST", "unix:///podman.sock"),
        ]);
        let resolver = SocketResolver::new(&env);
        assert_eq!(
            resolver.resolve_from_env(),
            Some(String::from("unix:///docker.sock"))
        );
    }

    #[rstest]
    fn resolver_prefers_docker_host_over_container_host() {
        let env = env_with_vars(&[
            ("DOCKER_HOST", "unix:///docker.sock"),
            ("CONTAINER_HOST", "unix:///container.sock"),
        ]);
        let resolver = SocketResolver::new(&env);
        assert_eq!(
            resolver.resolve_from_env(),
            Some(String::from("unix:///docker.sock"))
        );
    }

    #[rstest]
    fn resolver_prefers_container_host_over_podman_host() {
        let env = env_with_vars(&[
            ("CONTAINER_HOST", "unix:///container.sock"),
            ("PODMAN_HOST", "unix:///podman.sock"),
        ]);
        let resolver = SocketResolver::new(&env);
        assert_eq!(
            resolver.resolve_from_env(),
            Some(String::from("unix:///container.sock"))
        );
    }

    #[rstest]
    fn resolver_skips_empty_values() {
        let env = env_with_vars(&[("DOCKER_HOST", ""), ("PODMAN_HOST", "unix:///podman.sock")]);
        let resolver = SocketResolver::new(&env);
        assert_eq!(
            resolver.resolve_from_env(),
            Some(String::from("unix:///podman.sock"))
        );
    }

    #[rstest]
    fn resolver_skips_all_empty_values() {
        let env = env_with_vars(&[
            ("DOCKER_HOST", ""),
            ("CONTAINER_HOST", ""),
            ("PODMAN_HOST", ""),
        ]);
        let resolver = SocketResolver::new(&env);
        assert!(resolver.resolve_from_env().is_none());
    }

    #[rstest]
    #[cfg(unix)]
    fn default_socket_is_unix_socket() {
        assert_eq!(
            SocketResolver::<MockEnv>::default_socket(),
            "unix:///var/run/docker.sock"
        );
    }

    #[rstest]
    #[cfg(windows)]
    fn default_socket_is_named_pipe() {
        assert_eq!(
            SocketResolver::<MockEnv>::default_socket(),
            "npipe:////./pipe/docker_engine"
        );
    }

    #[rstest]
    fn resolve_socket_uses_config_when_provided() {
        let env = empty_env();
        let resolver = SocketResolver::new(&env);
        let socket = EngineConnector::resolve_socket(Some("unix:///config.sock"), &resolver);
        assert_eq!(socket, "unix:///config.sock");
    }

    #[rstest]
    fn resolve_socket_uses_env_when_config_is_none() {
        let env = env_with_docker_host("unix:///custom/docker.sock");
        let resolver = SocketResolver::new(&env);
        let socket = EngineConnector::resolve_socket(None, &resolver);
        assert_eq!(socket, "unix:///custom/docker.sock");
    }

    #[rstest]
    #[cfg(unix)]
    fn resolve_socket_uses_default_when_no_source_available() {
        let env = empty_env();
        let resolver = SocketResolver::new(&env);
        let socket = EngineConnector::resolve_socket(None, &resolver);
        assert_eq!(socket, "unix:///var/run/docker.sock");
    }

    #[rstest]
    fn resolve_socket_config_takes_precedence_over_env() {
        let env = env_with_docker_host("unix:///docker.sock");
        let resolver = SocketResolver::new(&env);
        let socket = EngineConnector::resolve_socket(Some("unix:///config.sock"), &resolver);
        assert_eq!(socket, "unix:///config.sock");
    }
}
