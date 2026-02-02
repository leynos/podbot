//! Health check step definitions for BDD tests.
//!
//! This module contains the step definitions related to container engine
//! health checks. It was extracted from the main helpers module to keep
//! file sizes manageable.

use mockable::MockEnv;
use podbot::engine::{EngineConnector, SocketResolver};
use podbot::error::{ContainerError, PodbotError};
use rstest_bdd_macros::{given, then, when};

use super::{EngineConnectionState, HealthCheckOutcome, StepResult};

// =============================================================================
// Helper functions
// =============================================================================

/// Returns true if the error message indicates no container daemon is available.
///
/// This predicate checks for common error patterns across platforms:
/// - Unix: "No such file or directory"
/// - Windows: "The system cannot find the file specified"
/// - Connection refused (daemon not running)
/// - "failed to connect" wraps the underlying error
fn is_daemon_unavailable(msg: &str) -> bool {
    msg.contains("No such file")
        || msg.contains("cannot find the file")
        || msg.contains("connection refused")
        || msg.contains("Connection refused")
        || msg.contains("failed to connect")
}

/// Check if the error is a health check timeout by matching on the error variant.
const fn is_health_check_timeout(err: &PodbotError) -> bool {
    matches!(
        err,
        PodbotError::Container(ContainerError::HealthCheckTimeout { .. })
    )
}

/// Returns a platform-appropriate non-existent socket path for testing.
///
/// On Unix, returns a unix:// socket path.
/// On Windows, returns an npipe:// named pipe path.
fn nonexistent_socket_path() -> String {
    #[cfg(unix)]
    {
        String::from("unix:///nonexistent/podbot-test.sock")
    }
    #[cfg(windows)]
    {
        String::from("npipe:////./pipe/nonexistent-podbot-test")
    }
}

/// Simulation mode for health check attempts.
///
/// Determines how the health check step should behave based on
/// the preconditions set by "Given" steps.
enum SimulationMode {
    /// Check the default socket normally.
    Normal,
    /// Simulate a non-responding engine using a non-existent socket.
    NotResponding,
    /// Simulate a slow engine (requires skip - not implementable in tests).
    SlowEngine,
}

impl SimulationMode {
    /// Determine the simulation mode from the current state.
    fn from_state(state: &EngineConnectionState) -> Self {
        let not_responding = state.simulate_not_responding.get().unwrap_or(false);
        let slow = state.simulate_slow_engine.get().unwrap_or(false);

        if not_responding {
            Self::NotResponding
        } else if slow {
            Self::SlowEngine
        } else {
            Self::Normal
        }
    }
}

/// Execute a health check and record the outcome.
fn execute_health_check_and_record(
    state: &EngineConnectionState,
    socket: &str,
) -> StepResult<Option<String>> {
    let rt = tokio::runtime::Runtime::new().map_err(|_| "failed to create tokio runtime")?;
    let result = rt.block_on(async { EngineConnector::connect_and_verify_async(socket).await });

    match result {
        Ok(_) => {
            state.health_check_outcome.set(HealthCheckOutcome::Success);
            Ok(None)
        }
        Err(e) => {
            let msg = e.to_string();
            if is_health_check_timeout(&e) {
                state.health_check_outcome.set(HealthCheckOutcome::Timeout);
            } else {
                state
                    .health_check_outcome
                    .set(HealthCheckOutcome::Failed(msg.clone()));
            }
            Ok(Some(msg))
        }
    }
}

// =============================================================================
// Given step definitions
// =============================================================================

/// Mark the scenario as expecting an available container engine daemon.
///
/// Sets up state indicating that a real container daemon should be running.
/// The actual connectivity check happens in the "When" step.
#[given("a container engine is available")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
pub fn container_engine_is_available(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    // Mark that we expect a real daemon to be available.
    // The actual check happens in the "When" step.
    engine_connection_state.simulate_not_responding.set(false);
    engine_connection_state.simulate_slow_engine.set(false);
    Ok(())
}

/// Mark the scenario as simulating a non-responding container engine.
///
/// Sets up state indicating that connection attempts should fail because
/// the daemon is not available (e.g., socket doesn't exist).
#[given("the container engine is not responding")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
pub fn container_engine_is_not_responding(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    engine_connection_state.simulate_not_responding.set(true);
    Ok(())
}

/// Mark the scenario as simulating a slow container engine.
///
/// Sets up state indicating that the engine responds too slowly,
/// causing health check timeouts.
#[given("the container engine is slow to respond")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
pub fn container_engine_is_slow_to_respond(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    engine_connection_state.simulate_slow_engine.set(true);
    Ok(())
}

// =============================================================================
// When step definitions
// =============================================================================

/// Perform a health check against the default container engine socket.
///
/// Attempts to connect to the platform default socket and verify the engine
/// responds. If no daemon is available, the scenario is skipped rather than
/// failing.
#[when("a health check is performed")]
pub fn health_check_is_performed(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    // Try to connect to the default socket and perform a health check.
    // If no daemon is available, skip the scenario.
    let default_socket = SocketResolver::<MockEnv>::default_socket();

    if let Some(msg) = execute_health_check_and_record(engine_connection_state, default_socket)? {
        if is_daemon_unavailable(&msg) {
            rstest_bdd::skip!("no container daemon available at {}", default_socket);
        }
    }
    Ok(())
}

/// Attempt a health check based on the configured simulation mode.
///
/// Behavior depends on state set by "Given" steps:
/// - Normal mode: check the default socket
/// - Not responding mode: check a non-existent socket to simulate failure
/// - Slow mode: skip (timeout simulation requires external setup)
#[when("a health check is attempted")]
pub fn health_check_is_attempted(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    match SimulationMode::from_state(engine_connection_state) {
        SimulationMode::NotResponding => {
            // Use a non-existent socket to simulate a non-responding engine.
            let socket = nonexistent_socket_path();
            execute_health_check_and_record(engine_connection_state, &socket)?;
        }
        SimulationMode::SlowEngine => {
            // Simulating a slow engine is difficult without a real slow endpoint.
            rstest_bdd::skip!("timeout simulation requires a controllable slow endpoint");
        }
        SimulationMode::Normal => {
            // Normal health check attempt against the default socket.
            let default_socket = SocketResolver::<MockEnv>::default_socket();
            execute_health_check_and_record(engine_connection_state, default_socket)?;
        }
    }
    Ok(())
}

// =============================================================================
// Then step definitions
// =============================================================================

/// Assert that the health check completed successfully.
///
/// Verifies the recorded outcome indicates the container engine responded
/// to the ping request within the timeout period.
#[then("the health check succeeds")]
pub fn health_check_succeeds(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
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

/// Assert that a health check failure error was returned.
///
/// Accepts both explicit failures and timeouts, as both indicate the engine
/// was not healthy.
#[then("a health check failure error is returned")]
pub fn health_check_failure_error_is_returned(
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

/// Assert that a health check timeout error was returned.
///
/// Specifically requires the `HealthCheckTimeout` variant, not a general
/// connection failure.
#[then("a health check timeout error is returned")]
pub fn health_check_timeout_error_is_returned(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let outcome = engine_connection_state
        .health_check_outcome
        .get()
        .ok_or("health check outcome should be set")?;

    match outcome {
        HealthCheckOutcome::Timeout => Ok(()),
        HealthCheckOutcome::Success => Err("Expected health check to timeout, but it succeeded"),
        HealthCheckOutcome::Failed(_) => Err("Expected timeout error, but got a different failure"),
    }
}
