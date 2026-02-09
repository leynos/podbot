//! Unit tests for TCP and HTTP connection paths.
//!
//! These tests verify that TCP endpoints (`tcp://`), HTTP endpoints
//! (`http://`), and HTTPS endpoints (`https://`) are correctly handled
//! by the `EngineConnector`, including scheme rewriting, client creation,
//! socket resolution, and fallback behaviour.
//!
//! All HTTP-compatible endpoints use Bollard's `connect_with_http`, which
//! creates the client configuration synchronously without validating
//! connectivity. If Bollard changes to validate endpoints eagerly, these
//! tests may start failing.

use mockable::MockEnv;
use rstest::{fixture, rstest};

use super::super::{EngineConnector, SocketResolver};
use super::env_with_docker_host;

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

// =============================================================================
// EngineConnector::connect — TCP and HTTP endpoint creation
// =============================================================================

#[rstest]
#[case::tcp_with_hostname("tcp://host:2375")]
#[case::tcp_with_ip("tcp://192.168.1.100:2376")]
#[case::http_endpoint("http://remotehost:2375")]
#[case::https_endpoint("https://remotehost:2376")]
#[case::tcp_with_fqdn("tcp://docker.example.com:2375")]
#[case::tcp_with_ipv4("tcp://10.0.0.1:2375")]
fn connect_http_compatible_endpoints_creates_client(#[case] endpoint: &str) {
    // HTTP-compatible endpoints (http://, https://, tcp://) use
    // Docker::connect_with_http which is synchronous and does not
    // validate connectivity at construction time.
    let result = EngineConnector::connect(endpoint);
    result.unwrap_or_else(|_| panic!("connect {endpoint} should create client"));
}

#[rstest]
fn connect_tcp_rewrites_scheme_to_http() {
    // tcp:// must be rewritten to http:// for Bollard. If the rewrite
    // did not occur, Bollard would reject the scheme. Success proves
    // the rewrite happened.
    let result = EngineConnector::connect("tcp://localhost:2375");
    assert!(
        result.is_ok(),
        concat!(
            "tcp:// endpoint should succeed (proving tcp->http rewrite); ",
            "got error: {:?}"
        ),
        result.err()
    );
}

// =============================================================================
// SocketResolver — TCP endpoints from environment variables
// =============================================================================

#[rstest]
fn resolver_returns_tcp_endpoint_from_docker_host() {
    let env = env_with_docker_host("tcp://remotehost:2375");
    let resolver = SocketResolver::new(&env);
    assert_eq!(
        resolver.resolve_from_env(),
        Some(String::from("tcp://remotehost:2375"))
    );
}

// =============================================================================
// EngineConnector::resolve_socket — TCP endpoints
// =============================================================================

#[rstest]
fn resolve_socket_uses_tcp_endpoint_from_config(empty_env: MockEnv) {
    let resolver = SocketResolver::new(&empty_env);
    let socket = EngineConnector::resolve_socket(Some("tcp://remotehost:2375"), &resolver);
    assert_eq!(socket, "tcp://remotehost:2375");
}

#[rstest]
fn resolve_socket_uses_tcp_endpoint_from_env() {
    let env = env_with_docker_host("tcp://192.168.1.100:2376");
    let resolver = SocketResolver::new(&env);
    let socket = EngineConnector::resolve_socket(None::<&str>, &resolver);
    assert_eq!(socket, "tcp://192.168.1.100:2376");
}

// =============================================================================
// EngineConnector::connect_with_fallback — TCP endpoints
// =============================================================================

#[rstest]
fn connect_with_fallback_uses_tcp_from_config(empty_env: MockEnv) {
    let resolver = SocketResolver::new(&empty_env);
    let result = EngineConnector::connect_with_fallback(Some("tcp://remotehost:2375"), &resolver);
    result.expect("connect_with_fallback tcp should succeed");
}

#[rstest]
fn connect_with_fallback_uses_tcp_from_env() {
    let env = env_with_docker_host("tcp://192.168.1.100:2376");
    let resolver = SocketResolver::new(&env);
    let result = EngineConnector::connect_with_fallback(None::<&str>, &resolver);
    result.expect("connect_with_fallback tcp from env should succeed");
}
