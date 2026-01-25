//! Behavioural test helpers for container engine connection.

// rstest-bdd macros generate internal code that triggers these lints for unused state parameters
#![allow(
    clippy::used_underscore_binding,
    reason = "rstest-bdd requires state parameter in macro-generated code"
)]
#![allow(
    non_snake_case,
    reason = "rstest-bdd generates non-snake-case internal variables"
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mockable::MockEnv;
use podbot::engine::{EngineConnector, SocketResolver};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, then, when};

/// Thread-safe environment variable storage for BDD tests.
type EnvVars = Arc<Mutex<HashMap<String, String>>>;

/// State shared across engine connection test scenarios.
#[derive(Default, ScenarioState)]
pub struct EngineConnectionState {
    /// The environment variables to mock.
    env_vars: Slot<EnvVars>,
    /// The configured socket from configuration (CLI, config file, `PODBOT_ENGINE_SOCKET`).
    config_socket: Slot<Option<String>>,
    /// The resolved socket endpoint.
    resolved_socket: Slot<String>,
}

/// Fixture providing a fresh engine connection state.
#[fixture]
pub fn engine_connection_state() -> EngineConnectionState {
    let state = EngineConnectionState::default();
    state.env_vars.set(Arc::new(Mutex::new(HashMap::new())));
    state
}

/// Helper to get the env vars map.
#[expect(clippy::expect_used, reason = "test helper - panics are acceptable")]
fn get_env_vars(state: &EngineConnectionState) -> EnvVars {
    state
        .env_vars
        .get()
        .expect("env_vars should be initialized")
}

/// Helper to set an environment variable.
#[expect(clippy::expect_used, reason = "test helper - panics are acceptable")]
fn set_env_var(state: &EngineConnectionState, key: &str, value: &str) {
    let env_vars = get_env_vars(state);
    let mut vars = env_vars.lock().expect("mutex should not be poisoned");
    vars.insert(String::from(key), String::from(value));
}

/// Creates a `MockEnv` from the current state.
#[expect(clippy::expect_used, reason = "test helper - panics are acceptable")]
fn create_mock_env(state: &EngineConnectionState) -> MockEnv {
    let env_vars = get_env_vars(state);
    let vars = env_vars
        .lock()
        .expect("mutex should not be poisoned")
        .clone();

    let mut mock = MockEnv::new();
    mock.expect_string()
        .returning(move |key| vars.get(key).cloned());
    mock
}

// Given step definitions

#[given("no engine socket is configured")]
fn no_engine_socket_configured(engine_connection_state: &EngineConnectionState) {
    engine_connection_state.config_socket.set(None);
}

#[given("engine socket is configured as {socket}")]
fn engine_socket_configured_as(engine_connection_state: &EngineConnectionState, socket: String) {
    engine_connection_state.config_socket.set(Some(socket));
}

#[given("DOCKER_HOST is set to {value}")]
fn docker_host_is_set_to(engine_connection_state: &EngineConnectionState, value: String) {
    set_env_var(engine_connection_state, "DOCKER_HOST", &value);
}

#[given("DOCKER_HOST is empty")]
fn docker_host_is_empty(engine_connection_state: &EngineConnectionState) {
    set_env_var(engine_connection_state, "DOCKER_HOST", "");
}

#[given("DOCKER_HOST is not set")]
#[expect(
    unused_variables,
    reason = "rstest-bdd requires parameter to match fixture name"
)]
fn docker_host_is_not_set(engine_connection_state: &EngineConnectionState) {
    // No-op: variables not in the map are treated as unset
}

#[given("CONTAINER_HOST is set to {value}")]
fn container_host_is_set_to(engine_connection_state: &EngineConnectionState, value: String) {
    set_env_var(engine_connection_state, "CONTAINER_HOST", &value);
}

#[given("CONTAINER_HOST is not set")]
#[expect(
    unused_variables,
    reason = "rstest-bdd requires parameter to match fixture name"
)]
fn container_host_is_not_set(engine_connection_state: &EngineConnectionState) {
    // No-op: variables not in the map are treated as unset
}

#[given("PODMAN_HOST is set to {value}")]
fn podman_host_is_set_to(engine_connection_state: &EngineConnectionState, value: String) {
    set_env_var(engine_connection_state, "PODMAN_HOST", &value);
}

#[given("PODMAN_HOST is not set")]
#[expect(
    unused_variables,
    reason = "rstest-bdd requires parameter to match fixture name"
)]
fn podman_host_is_not_set(engine_connection_state: &EngineConnectionState) {
    // No-op: variables not in the map are treated as unset
}

// When step definitions

#[when("the socket is resolved")]
fn the_socket_is_resolved(engine_connection_state: &EngineConnectionState) {
    let env = create_mock_env(engine_connection_state);
    let resolver = SocketResolver::new(&env);
    let config_socket = engine_connection_state.config_socket.get().flatten();
    let socket = EngineConnector::resolve_socket(config_socket.as_deref(), &resolver);
    engine_connection_state.resolved_socket.set(socket);
}

// Then step definitions

#[then("the resolved socket is {expected}")]
#[expect(clippy::expect_used, reason = "test step - panics are acceptable")]
fn the_resolved_socket_is(engine_connection_state: &EngineConnectionState, expected: String) {
    let resolved = engine_connection_state
        .resolved_socket
        .get()
        .expect("resolved socket should be set");
    assert_eq!(
        resolved, expected,
        "Expected resolved socket to be '{}', but got '{}'",
        expected, resolved
    );
}

#[then("the socket resolves to the platform default")]
#[expect(clippy::expect_used, reason = "test step - panics are acceptable")]
fn the_socket_resolves_to_platform_default(engine_connection_state: &EngineConnectionState) {
    let resolved = engine_connection_state
        .resolved_socket
        .get()
        .expect("resolved socket should be set");
    let default = SocketResolver::<MockEnv>::default_socket();
    assert_eq!(
        resolved, default,
        "Expected resolved socket to be platform default '{}', but got '{}'",
        default, resolved
    );
}
