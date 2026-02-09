//! Unit tests for socket resolution and container engine connection.
//!
//! This module tests the `SocketResolver` and `EngineConnector` types,
//! covering environment variable resolution, fallback behaviour, and
//! connection establishment for various socket types.

use mockable::MockEnv;
use rstest::{fixture, rstest};

use super::{EngineConnector, SocketResolver};
use crate::error::{ContainerError, PodbotError};

// =============================================================================
// Fixtures and helpers
// =============================================================================

/// Fixture providing a `MockEnv` that returns `None` for all environment
/// variable queries.
#[fixture]
fn empty_env() -> MockEnv {
    let mut env = MockEnv::new();
    env.expect_string().returning(|_| None);
    env
}

/// Fixture providing a `MockEnv` with `DOCKER_HOST` set to an empty string
/// and `PODMAN_HOST` set to a valid socket path.
#[fixture]
fn docker_empty_podman_fallback_env() -> MockEnv {
    let mut env = MockEnv::new();
    env.expect_string().returning(|key| match key {
        "DOCKER_HOST" => Some(String::new()),
        "PODMAN_HOST" => Some(String::from("unix:///podman.sock")),
        _ => None,
    });
    env
}

/// Fixture providing a `MockEnv` with all socket environment variables set to
/// empty strings.
#[fixture]
fn all_empty_env_vars() -> MockEnv {
    let mut env = MockEnv::new();
    env.expect_string().returning(|key| match key {
        "DOCKER_HOST" | "CONTAINER_HOST" | "PODMAN_HOST" => Some(String::new()),
        _ => None,
    });
    env
}

/// Fixture providing a tokio runtime for async tests.
#[fixture]
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("runtime creation should succeed")
}

/// Helper function to create a `MockEnv` with `DOCKER_HOST` set to the
/// specified socket path.
pub(super) fn env_with_docker_host(socket: &str) -> MockEnv {
    let socket_path = String::from(socket);
    let mut env = MockEnv::new();
    env.expect_string().returning(move |key| {
        if key == "DOCKER_HOST" {
            Some(socket_path.clone())
        } else {
            None
        }
    });
    env
}

// =============================================================================
// SocketResolver tests
// =============================================================================

#[rstest]
fn resolver_returns_none_when_no_env_vars_set(empty_env: MockEnv) {
    let resolver = SocketResolver::new(&empty_env);
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
#[case::docker_host_only(
    "respects DOCKER_HOST",
    vec![("DOCKER_HOST", "unix:///docker.sock")],
    Some("unix:///docker.sock")
)]
#[case::container_host_only(
    "respects CONTAINER_HOST",
    vec![("CONTAINER_HOST", "unix:///container.sock")],
    Some("unix:///container.sock")
)]
#[case::podman_host_only(
    "respects PODMAN_HOST",
    vec![("PODMAN_HOST", "unix:///podman.sock")],
    Some("unix:///podman.sock")
)]
#[case::docker_over_podman(
    "prefers DOCKER_HOST over PODMAN_HOST",
    vec![("DOCKER_HOST", "unix:///docker.sock"), ("PODMAN_HOST", "unix:///podman.sock")],
    Some("unix:///docker.sock")
)]
#[case::docker_over_container(
    "prefers DOCKER_HOST over CONTAINER_HOST",
    vec![("DOCKER_HOST", "unix:///docker.sock"), ("CONTAINER_HOST", "unix:///container.sock")],
    Some("unix:///docker.sock")
)]
#[case::container_over_podman(
    "prefers CONTAINER_HOST over PODMAN_HOST",
    vec![("CONTAINER_HOST", "unix:///container.sock"), ("PODMAN_HOST", "unix:///podman.sock")],
    Some("unix:///container.sock")
)]
fn resolver_env_var_resolution(
    #[case] description: &str,
    #[case] env_vars: Vec<(&str, &str)>,
    #[case] expected: Option<&str>,
) {
    // Build the MockEnv inline from parameterized data
    let owned_vars: Vec<(String, String)> = env_vars
        .into_iter()
        .map(|(k, v)| (String::from(k), String::from(v)))
        .collect();
    let mut env = MockEnv::new();
    env.expect_string().returning(move |key| {
        owned_vars
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    });
    let resolver = SocketResolver::new(&env);
    assert_eq!(
        resolver.resolve_from_env(),
        expected.map(String::from),
        "mismatch for case: {description}"
    );
}

#[rstest]
fn resolver_skips_empty_values(docker_empty_podman_fallback_env: MockEnv) {
    let resolver = SocketResolver::new(&docker_empty_podman_fallback_env);
    assert_eq!(
        resolver.resolve_from_env(),
        Some(String::from("unix:///podman.sock"))
    );
}

#[rstest]
fn resolver_skips_all_empty_values(all_empty_env_vars: MockEnv) {
    let resolver = SocketResolver::new(&all_empty_env_vars);
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

// =============================================================================
// EngineConnector::resolve_socket tests
// =============================================================================

#[rstest]
fn resolve_socket_uses_config_when_provided(empty_env: MockEnv) {
    let resolver = SocketResolver::new(&empty_env);
    let socket = EngineConnector::resolve_socket(Some("unix:///config.sock"), &resolver);
    assert_eq!(socket, "unix:///config.sock");
}

#[rstest]
fn resolve_socket_uses_env_when_config_is_none() {
    let env = env_with_docker_host("unix:///custom/docker.sock");
    let resolver = SocketResolver::new(&env);
    let socket = EngineConnector::resolve_socket(None::<&str>, &resolver);
    assert_eq!(socket, "unix:///custom/docker.sock");
}

#[rstest]
#[cfg(unix)]
fn resolve_socket_uses_default_when_no_source_available(empty_env: MockEnv) {
    let resolver = SocketResolver::new(&empty_env);
    let socket = EngineConnector::resolve_socket(None::<&str>, &resolver);
    assert_eq!(socket, "unix:///var/run/docker.sock");
}

#[rstest]
fn resolve_socket_config_takes_precedence_over_env() {
    let env = env_with_docker_host("unix:///docker.sock");
    let resolver = SocketResolver::new(&env);
    let socket = EngineConnector::resolve_socket(Some("unix:///config.sock"), &resolver);
    assert_eq!(socket, "unix:///config.sock");
}

#[rstest]
#[cfg(unix)]
fn resolve_socket_skips_empty_config(empty_env: MockEnv) {
    let resolver = SocketResolver::new(&empty_env);
    let socket = EngineConnector::resolve_socket(Some(""), &resolver);
    assert_eq!(socket, "unix:///var/run/docker.sock");
}

#[rstest]
fn resolve_socket_empty_config_falls_back_to_env() {
    let env = env_with_docker_host("unix:///docker.sock");
    let resolver = SocketResolver::new(&env);
    let socket = EngineConnector::resolve_socket(Some(""), &resolver);
    assert_eq!(socket, "unix:///docker.sock");
}

#[path = "tests_tcp.rs"]
mod tcp;

// =============================================================================
// EngineConnector::connect_and_verify tests
// =============================================================================

#[rstest]
fn connect_and_verify_propagates_connection_errors(runtime: tokio::runtime::Runtime) {
    // Using a non-existent Unix socket to trigger a connection error.
    // The actual error occurs during the connect phase, not the health check.
    let result = runtime.block_on(async {
        EngineConnector::connect_and_verify_async("unix:///nonexistent/socket.sock").await
    });

    // Connection to non-existent socket should fail with SocketNotFound variant
    // (error classification detects the NotFound IO error kind)
    let err = result.expect_err("connect to non-existent socket should fail");
    assert!(
        matches!(
            err,
            PodbotError::Container(ContainerError::SocketNotFound { .. })
        ),
        "expected SocketNotFound error variant, got: {err}"
    );
}

#[rstest]
#[cfg(unix)]
fn connect_and_verify_classifies_bare_path_socket_not_found(runtime: tokio::runtime::Runtime) {
    // Bare paths are normalized to unix:// URIs before connecting, and
    // classification should use that normalized URI to extract the path.
    let result = runtime.block_on(async {
        EngineConnector::connect_and_verify_async("/nonexistent/socket.sock").await
    });

    let err = result.expect_err("connect to non-existent bare socket path should fail");
    assert!(
        matches!(
            err,
            PodbotError::Container(ContainerError::SocketNotFound { ref path })
                if path.to_str() == Some("/nonexistent/socket.sock")
        ),
        "expected SocketNotFound with extracted path, got: {err}"
    );
}

#[rstest]
fn connect_with_fallback_and_verify_uses_resolved_socket(
    empty_env: MockEnv,
    runtime: tokio::runtime::Runtime,
) {
    // Verify that connect_with_fallback_and_verify resolves the socket correctly
    // before attempting connection. We use an explicit socket that will fail
    // to connect, but verify the resolution logic works by checking the socket
    // path appears in the error message.
    let resolver = SocketResolver::new(&empty_env);

    let result = runtime.block_on(async {
        EngineConnector::connect_with_fallback_and_verify_async(
            Some("unix:///nonexistent/test.sock"),
            &resolver,
        )
        .await
    });

    // Connection should fail (no daemon at this path)
    assert!(
        result.is_err(),
        "connect to non-existent socket should fail"
    );

    // Verify the error references the config-provided socket path, confirming
    // that config takes precedence over env and default.
    let err_msg = result
        .expect_err("expected connection error when connecting to non-existent socket")
        .to_string();
    assert!(
        err_msg.contains("nonexistent/test.sock"),
        "error should indicate which socket was used (config-resolved path), got: {err_msg}"
    );
}

#[rstest]
fn connect_with_fallback_and_verify_falls_back_to_env(runtime: tokio::runtime::Runtime) {
    // Verify that when config is None, the resolver's environment fallback is used.
    let env = env_with_docker_host("unix:///env/docker.sock");
    let resolver = SocketResolver::new(&env);

    let result = runtime.block_on(async {
        EngineConnector::connect_with_fallback_and_verify_async(None::<&str>, &resolver).await
    });

    // Connection will fail (no daemon), but verify the env path was used
    assert!(
        result.is_err(),
        "connect to non-existent socket should fail"
    );

    // Verify the error references the environment-provided socket path,
    // confirming that the fallback to DOCKER_HOST worked.
    let err_msg = result
        .expect_err("expected connection error when connecting to non-existent socket")
        .to_string();
    assert!(
        err_msg.contains("env/docker.sock"),
        "error should indicate which socket was used (env-resolved path), got: {err_msg}"
    );
}
