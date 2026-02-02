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

#[when("a health check is attempted")]
pub fn health_check_is_attempted(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let simulate_not_responding = engine_connection_state
        .simulate_not_responding
        .get()
        .unwrap_or(false);
    let simulate_slow = engine_connection_state
        .simulate_slow_engine
        .get()
        .unwrap_or(false);

    if simulate_not_responding {
        // Use a non-existent socket to simulate a non-responding engine.
        // Construct a platform-appropriate non-existent socket path.
        let socket = nonexistent_socket_path();
        execute_health_check_and_record(engine_connection_state, &socket)?;
    } else if simulate_slow {
        // Simulating a slow engine is difficult without a real slow endpoint.
        // For now, we document this behaviour and skip the actual timeout test.
        rstest_bdd::skip!("timeout simulation requires a controllable slow endpoint");
    } else {
        // Normal health check attempt
        let default_socket = SocketResolver::<MockEnv>::default_socket();
        execute_health_check_and_record(engine_connection_state, default_socket)?;
    }
    Ok(())
}

// =============================================================================
// Then step definitions
// =============================================================================

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
