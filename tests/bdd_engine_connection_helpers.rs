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

/// Step result type for BDD tests, using a static string for errors.
type StepResult<T> = Result<T, &'static str>;

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
    health_check_outcome: Slot<HealthCheckOutcome>,
    /// Whether the scenario simulates a non-responding engine.
    simulate_not_responding: Slot<bool>,
    /// Whether the scenario simulates a slow engine.
    simulate_slow_engine: Slot<bool>,
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

// Given step definitions

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

// When step definitions

#[when("the socket is resolved")]
fn the_socket_is_resolved(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    let env = create_mock_env(engine_connection_state)?;
    let resolver = SocketResolver::new(&env);
    let config_socket = engine_connection_state.config_socket.get().flatten();
    let socket = EngineConnector::resolve_socket(config_socket.as_deref(), &resolver);
    engine_connection_state.resolved_socket.set(socket);
    Ok(())
}

// Then step definitions

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

// =============================================================================
// Health check step definitions
// =============================================================================

#[given("a container engine is available")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
fn container_engine_is_available(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    // Mark that we expect a real daemon to be available.
    // The actual check happens in the "When" step.
    engine_connection_state.simulate_not_responding.set(false);
    engine_connection_state.simulate_slow_engine.set(false);
    Ok(())
}

#[given("the container engine is not responding")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
fn container_engine_is_not_responding(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    engine_connection_state.simulate_not_responding.set(true);
    Ok(())
}

#[given("the container engine is slow to respond")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
fn container_engine_is_slow_to_respond(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    engine_connection_state.simulate_slow_engine.set(true);
    Ok(())
}

#[when("a health check is performed")]
fn health_check_is_performed(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    // Try to connect to the default socket and perform a health check.
    // If no daemon is available, skip the scenario.
    let default_socket = SocketResolver::<MockEnv>::default_socket();

    let rt = tokio::runtime::Runtime::new().map_err(|_| "failed to create tokio runtime")?;
    let result = rt.block_on(async { EngineConnector::connect_and_verify(default_socket) });

    match result {
        Ok(_) => {
            engine_connection_state
                .health_check_outcome
                .set(HealthCheckOutcome::Success);
        }
        Err(e) => {
            let msg = e.to_string();
            // If the connection fails because no daemon is running, skip the test.
            // Common error messages for missing daemons:
            // - Unix: "No such file or directory"
            // - Windows: "The system cannot find the file specified"
            // - Connection refused (daemon not running)
            // - "failed to connect" wraps the underlying error
            let should_skip = msg.contains("No such file")
                || msg.contains("cannot find the file")
                || msg.contains("connection refused")
                || msg.contains("Connection refused")
                || msg.contains("failed to connect");
            if should_skip {
                rstest_bdd::skip!("no container daemon available at {}", default_socket);
            } else {
                engine_connection_state
                    .health_check_outcome
                    .set(HealthCheckOutcome::Failed(msg));
            }
        }
    }
    Ok(())
}

#[when("a health check is attempted")]
fn health_check_is_attempted(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    let simulate_not_responding = engine_connection_state
        .simulate_not_responding
        .get()
        .unwrap_or(false);
    let simulate_slow = engine_connection_state
        .simulate_slow_engine
        .get()
        .unwrap_or(false);

    if simulate_not_responding {
        // Use a non-existent socket to simulate a non-responding engine
        let rt = tokio::runtime::Runtime::new().map_err(|_| "failed to create tokio runtime")?;
        let result = rt.block_on(async {
            EngineConnector::connect_and_verify("unix:///nonexistent/docker.sock")
        });

        match result {
            Ok(_) => {
                engine_connection_state
                    .health_check_outcome
                    .set(HealthCheckOutcome::Success);
            }
            Err(e) => {
                engine_connection_state
                    .health_check_outcome
                    .set(HealthCheckOutcome::Failed(e.to_string()));
            }
        }
    } else if simulate_slow {
        // Simulating a slow engine is difficult without a real slow endpoint.
        // For now, we document this behaviour and skip the actual timeout test.
        rstest_bdd::skip!("timeout simulation requires a controllable slow endpoint");
    } else {
        // Normal health check attempt
        let default_socket = SocketResolver::<MockEnv>::default_socket();
        let rt = tokio::runtime::Runtime::new().map_err(|_| "failed to create tokio runtime")?;
        let result = rt.block_on(async { EngineConnector::connect_and_verify(default_socket) });

        match result {
            Ok(_) => {
                engine_connection_state
                    .health_check_outcome
                    .set(HealthCheckOutcome::Success);
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("timed out") {
                    engine_connection_state
                        .health_check_outcome
                        .set(HealthCheckOutcome::Timeout);
                } else {
                    engine_connection_state
                        .health_check_outcome
                        .set(HealthCheckOutcome::Failed(msg));
                }
            }
        }
    }
    Ok(())
}

#[then("the health check succeeds")]
fn health_check_succeeds(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    let outcome = engine_connection_state
        .health_check_outcome
        .get()
        .ok_or("health check outcome should be set")?;

    match outcome {
        HealthCheckOutcome::Success => Ok(()),
        HealthCheckOutcome::Failed(_) => Err("Expected health check to succeed, but it failed"),
        HealthCheckOutcome::Timeout => Err("Expected health check to succeed, but it timed out"),
    }
}

#[then("a health check failure error is returned")]
fn health_check_failure_error_is_returned(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let outcome = engine_connection_state
        .health_check_outcome
        .get()
        .ok_or("health check outcome should be set")?;

    match outcome {
        HealthCheckOutcome::Failed(_) | HealthCheckOutcome::Timeout => {
            // A timeout is also a kind of failure, so accept it here
            Ok(())
        }
        HealthCheckOutcome::Success => Err("Expected health check to fail, but it succeeded"),
    }
}

#[then("a health check timeout error is returned")]
fn health_check_timeout_error_is_returned(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let outcome = engine_connection_state
        .health_check_outcome
        .get()
        .ok_or("health check outcome should be set")?;

    match outcome {
        HealthCheckOutcome::Timeout => Ok(()),
        HealthCheckOutcome::Success => Err("Expected health check to timeout, but it succeeded"),
        HealthCheckOutcome::Failed(msg) => {
            // Check if the failure message mentions timeout
            if msg.contains("timed out") {
                Ok(())
            } else {
                Err("Expected timeout error, but got a different failure")
            }
        }
    }
}
