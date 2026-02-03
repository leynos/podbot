//! Behavioural test helpers for container engine connection.
//!
//! This module provides step definitions and state management for BDD tests
//! covering socket resolution and health check functionality.

mod health_check_steps;
mod permission_error_steps;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mockable::MockEnv;
use podbot::engine::{EngineConnector, SocketResolver};
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, then, when};

// Re-export step definitions so they are visible to rstest-bdd macros.
#[expect(
    unused_imports,
    reason = "rstest-bdd discovers step functions via attributes, not runtime usage"
)]
pub use health_check_steps::*;
#[expect(
    unused_imports,
    reason = "rstest-bdd discovers step functions via attributes, not runtime usage"
)]
pub use permission_error_steps::*;

/// Step result type for BDD tests, using a static string for errors.
pub type StepResult<T> = Result<T, &'static str>;

/// Thread-safe environment variable storage for BDD tests.
type EnvVars = Arc<Mutex<HashMap<String, String>>>;

/// Represents the outcome of a health check operation.
#[derive(Clone)]
pub enum HealthCheckOutcome {
    /// Health check succeeded.
    Success,
    /// Health check failed with an error message.
    Failed(String),
    /// Health check timed out.
    Timeout,
}

/// Represents the outcome of a connection attempt (for error testing).
#[derive(Clone)]
pub enum ConnectionOutcome {
    /// Connection succeeded.
    Success,
    /// Permission denied error with the socket path.
    PermissionDenied(String),
    /// Socket not found error with the socket path.
    SocketNotFound(String),
    /// Other connection failure with the error message.
    OtherError(String),
}

/// State shared across engine connection test scenarios.
#[derive(Default, ScenarioState)]
pub struct EngineConnectionState {
    /// The environment variables to mock.
    env_vars: Slot<EnvVars>,
    /// The configured socket from configuration (CLI, config file, `PODBOT_ENGINE_SOCKET`).
    config_socket: Slot<Option<String>>,
    /// The resolved socket endpoint.
    resolved_socket: Slot<String>,
    /// The result of a health check operation.
    pub health_check_outcome: Slot<HealthCheckOutcome>,
    /// Whether the scenario simulates a non-responding engine.
    pub simulate_not_responding: Slot<bool>,
    /// Whether the scenario simulates a slow engine.
    pub simulate_slow_engine: Slot<bool>,
    /// The socket path to test against (for error testing).
    pub test_socket_path: Slot<String>,
    /// The outcome of a connection attempt (for error testing).
    pub connection_outcome: Slot<ConnectionOutcome>,
}

/// Fixture providing a fresh engine connection state.
#[fixture]
pub fn engine_connection_state() -> EngineConnectionState {
    let state = EngineConnectionState::default();
    state.env_vars.set(Arc::new(Mutex::new(HashMap::new())));
    state
}

/// Helper to get the env vars map.
fn get_env_vars(state: &EngineConnectionState) -> Result<EnvVars, &'static str> {
    state.env_vars.get().ok_or("env_vars should be initialised")
}

/// Helper to set an environment variable.
fn set_env_var(state: &EngineConnectionState, key: &str, value: &str) -> Result<(), &'static str> {
    let env_vars = get_env_vars(state)?;
    let mut vars = env_vars.lock().map_err(|_| "mutex poisoned")?;
    vars.insert(String::from(key), String::from(value));
    Ok(())
}

/// Creates a `MockEnv` from the current state.
///
/// **Note:** The returned `MockEnv` captures a snapshot of the environment
/// variables at the time of creation. Any `set_env_var` calls made after
/// `create_mock_env` will not be visible to the mock. This is intentional:
/// in BDD scenarios, all "Given" steps (which call `set_env_var`) complete
/// before the "When" step (which calls `create_mock_env`).
fn create_mock_env(state: &EngineConnectionState) -> Result<MockEnv, &'static str> {
    let env_vars = get_env_vars(state)?;
    let vars = env_vars.lock().map_err(|_| "mutex poisoned")?.clone();

    let mut mock = MockEnv::new();
    mock.expect_string()
        .returning(move |key| vars.get(key).cloned());
    Ok(mock)
}

// =============================================================================
// Socket resolution step definitions
// =============================================================================

#[given("no engine socket is configured")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
fn no_engine_socket_configured(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    engine_connection_state.config_socket.set(None);
    Ok(())
}

#[given("engine socket is configured as {socket}")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
fn engine_socket_configured_as(
    engine_connection_state: &EngineConnectionState,
    socket: String,
) -> StepResult<()> {
    engine_connection_state.config_socket.set(Some(socket));
    Ok(())
}

#[given("DOCKER_HOST is set to {value}")]
fn docker_host_is_set_to(
    engine_connection_state: &EngineConnectionState,
    value: String,
) -> StepResult<()> {
    set_env_var(engine_connection_state, "DOCKER_HOST", &value)?;
    Ok(())
}

#[given("DOCKER_HOST is empty")]
fn docker_host_is_empty(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    set_env_var(engine_connection_state, "DOCKER_HOST", "")?;
    Ok(())
}

#[given("DOCKER_HOST is not set")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
#[expect(
    unused_variables,
    reason = "rstest-bdd requires parameter to match fixture name"
)]
fn docker_host_is_not_set(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    // No-op: variables not in the map are treated as unset
    Ok(())
}

#[given("CONTAINER_HOST is set to {value}")]
fn container_host_is_set_to(
    engine_connection_state: &EngineConnectionState,
    value: String,
) -> StepResult<()> {
    set_env_var(engine_connection_state, "CONTAINER_HOST", &value)?;
    Ok(())
}

#[given("CONTAINER_HOST is not set")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
#[expect(
    unused_variables,
    reason = "rstest-bdd requires parameter to match fixture name"
)]
fn container_host_is_not_set(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    // No-op: variables not in the map are treated as unset
    Ok(())
}

#[given("PODMAN_HOST is set to {value}")]
fn podman_host_is_set_to(
    engine_connection_state: &EngineConnectionState,
    value: String,
) -> StepResult<()> {
    set_env_var(engine_connection_state, "PODMAN_HOST", &value)?;
    Ok(())
}

#[given("PODMAN_HOST is not set")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
#[expect(
    unused_variables,
    reason = "rstest-bdd requires parameter to match fixture name"
)]
fn podman_host_is_not_set(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    // No-op: variables not in the map are treated as unset
    Ok(())
}

#[when("the socket is resolved")]
fn the_socket_is_resolved(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    let env = create_mock_env(engine_connection_state)?;
    let resolver = SocketResolver::new(&env);
    let config_socket = engine_connection_state.config_socket.get().flatten();
    let socket = EngineConnector::resolve_socket(config_socket.as_deref(), &resolver);
    engine_connection_state.resolved_socket.set(socket);
    Ok(())
}

#[then("the resolved socket is {expected}")]
fn the_resolved_socket_is(
    engine_connection_state: &EngineConnectionState,
    expected: String,
) -> StepResult<()> {
    let resolved = engine_connection_state
        .resolved_socket
        .get()
        .ok_or("resolved socket should be set")?;
    assert_eq!(
        resolved, expected,
        "Expected resolved socket to be '{}', but got '{}'",
        expected, resolved
    );
    Ok(())
}

#[then("the socket resolves to the platform default")]
fn the_socket_resolves_to_platform_default(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let resolved = engine_connection_state
        .resolved_socket
        .get()
        .ok_or("resolved socket should be set")?;
    let default = SocketResolver::<MockEnv>::default_socket();
    assert_eq!(
        resolved, default,
        "Expected resolved socket to be platform default '{}', but got '{}'",
        default, resolved
    );
    Ok(())
}
