//! Unit tests for socket resolution and container engine connection.
//!
//! This module tests the `SocketResolver` and `EngineConnector` types,
//! covering environment variable resolution, fallback behaviour, and
//! connection establishment for various socket types.

use mockable::MockEnv;
use rstest::{fixture, rstest};

use super::{EngineConnector, SocketResolver};

// =============================================================================
// Fixtures
// =============================================================================

/// Fixture providing a `MockEnv` that returns `None` for all environment
/// variable queries.
#[fixture]
fn empty_env() -> MockEnv {
    let mut env = MockEnv::new();
    env.expect_string().returning(|_| None);
    env
}

/// Fixture providing a `MockEnv` with `DOCKER_HOST` set to a custom socket path.
#[fixture]
fn docker_host_env() -> MockEnv {
    let mut env = MockEnv::new();
    env.expect_string().returning(|key| {
        if key == "DOCKER_HOST" {
            Some(String::from("unix:///custom/docker.sock"))
        } else {
            None
        }
    });
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

/// Fixture providing a `MockEnv` with `DOCKER_HOST` set to a socket path,
/// used for testing config precedence over environment.
#[fixture]
fn docker_host_for_precedence_env() -> MockEnv {
    let mut env = MockEnv::new();
    env.expect_string().returning(|key| {
        if key == "DOCKER_HOST" {
            Some(String::from("unix:///docker.sock"))
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
fn resolver_returns_docker_host_when_set(docker_host_env: MockEnv) {
    let resolver = SocketResolver::new(&docker_host_env);
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
fn resolve_socket_uses_env_when_config_is_none(docker_host_env: MockEnv) {
    let resolver = SocketResolver::new(&docker_host_env);
    let socket = EngineConnector::resolve_socket(None, &resolver);
    assert_eq!(socket, "unix:///custom/docker.sock");
}

#[rstest]
#[cfg(unix)]
fn resolve_socket_uses_default_when_no_source_available(empty_env: MockEnv) {
    let resolver = SocketResolver::new(&empty_env);
    let socket = EngineConnector::resolve_socket(None, &resolver);
    assert_eq!(socket, "unix:///var/run/docker.sock");
}

#[rstest]
fn resolve_socket_config_takes_precedence_over_env(docker_host_for_precedence_env: MockEnv) {
    let resolver = SocketResolver::new(&docker_host_for_precedence_env);
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
fn resolve_socket_empty_config_falls_back_to_env(docker_host_for_precedence_env: MockEnv) {
    let resolver = SocketResolver::new(&docker_host_for_precedence_env);
    let socket = EngineConnector::resolve_socket(Some(""), &resolver);
    assert_eq!(socket, "unix:///docker.sock");
}

// =============================================================================
// EngineConnector::connect tests
// =============================================================================

#[rstest]
fn connect_tcp_endpoint_creates_client() {
    // tcp:// endpoints are rewritten to http:// and use connect_with_http.
    // Bollard's connect_with_http is synchronous and just creates the client
    // configuration, so this should succeed without a real Docker daemon.
    //
    // Note: This test relies on Bollard's `connect_with_http` being synchronous
    // and not validating connectivity at construction time. If Bollard's behaviour
    // changes to validate endpoints eagerly, this test may start failing.
    let result = EngineConnector::connect("tcp://host:2375");
    result.expect("connect tcp://host:2375 should create client");
}

#[rstest]
fn connect_tcp_endpoint_with_ip_creates_client() {
    // Verify tcp:// works with IP addresses as well as hostnames.
    //
    // Note: This test relies on Bollard's `connect_with_http` being synchronous
    // and not validating connectivity at construction time. If Bollard's behaviour
    // changes to validate endpoints eagerly, this test may start failing.
    let result = EngineConnector::connect("tcp://192.168.1.100:2376");
    result.expect("connect tcp://192.168.1.100:2376 should create client");
}
