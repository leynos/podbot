//! Permission error step definitions for BDD tests.
//!
//! This module contains step definitions for testing socket permission
//! and not-found error handling scenarios.

use podbot::engine::EngineConnector;
use podbot::error::{ContainerError, PodbotError};
use rstest_bdd_macros::{given, then, when};

use super::{ConnectionOutcome, EngineConnectionState, StepResult};

// =============================================================================
// Given step definitions
// =============================================================================

/// Set up a socket path that typically requires elevated permissions.
///
/// Uses the system Docker socket which normally requires docker group
/// membership or root access.
#[given("a socket path that requires elevated permissions")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn socket_requires_elevated_permissions(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    engine_connection_state
        .test_socket_path
        .set(String::from("unix:///var/run/docker.sock"));
    Ok(())
}

/// Set up a socket path that does not exist.
///
/// Uses a path that is guaranteed not to exist on any system.
#[given("a socket path that does not exist")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn socket_does_not_exist(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    engine_connection_state
        .test_socket_path
        .set(String::from("unix:///nonexistent/podbot-test-socket.sock"));
    Ok(())
}

// =============================================================================
// When step definitions
// =============================================================================

/// Attempt a connection to the configured test socket path.
///
/// Stores the outcome (success or error type) in the state for later assertion.
#[when("a connection is attempted")]
fn connection_is_attempted(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    let socket = engine_connection_state
        .test_socket_path
        .get()
        .ok_or("test socket path should be set")?;

    let result = EngineConnector::connect(&socket);

    let outcome = match result {
        Ok(_) => ConnectionOutcome::Success,
        Err(PodbotError::Container(ContainerError::PermissionDenied { path })) => {
            ConnectionOutcome::PermissionDenied(path.display().to_string())
        }
        Err(PodbotError::Container(ContainerError::SocketNotFound { path })) => {
            ConnectionOutcome::SocketNotFound(path.display().to_string())
        }
        Err(e) => ConnectionOutcome::OtherError(e.to_string()),
    };

    engine_connection_state.connection_outcome.set(outcome);
    Ok(())
}

// =============================================================================
// Then step definitions
// =============================================================================

/// Assert that a permission denied error was returned.
///
/// Skips the test if the user has permissions or if the socket doesn't exist.
#[then("a permission denied error is returned")]
fn permission_denied_error_returned(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let outcome = engine_connection_state
        .connection_outcome
        .get()
        .ok_or("connection outcome should be set")?;

    match outcome {
        ConnectionOutcome::PermissionDenied(_) => Ok(()),
        ConnectionOutcome::Success => {
            rstest_bdd::skip!("user has permission to access the socket");
        }
        ConnectionOutcome::SocketNotFound(_) => {
            rstest_bdd::skip!("socket not found; daemon may not be running");
        }
        ConnectionOutcome::OtherError(msg) => Err(Box::leak(
            format!("expected PermissionDenied, got: {msg}").into_boxed_str(),
        )),
    }
}

/// Assert that a socket not found error was returned.
#[then("a socket not found error is returned")]
fn socket_not_found_error_returned(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let outcome = engine_connection_state
        .connection_outcome
        .get()
        .ok_or("connection outcome should be set")?;

    match outcome {
        ConnectionOutcome::SocketNotFound(_) => Ok(()),
        ConnectionOutcome::OtherError(msg) => Err(Box::leak(
            format!("expected SocketNotFound, got: {msg}").into_boxed_str(),
        )),
        ConnectionOutcome::Success => Err("expected error but connection succeeded"),
        ConnectionOutcome::PermissionDenied(path) => Err(Box::leak(
            format!("expected SocketNotFound, got PermissionDenied for: {path}").into_boxed_str(),
        )),
    }
}

/// Assert that the error message includes the socket path.
#[then("the error message includes the socket path")]
fn error_includes_socket_path(engine_connection_state: &EngineConnectionState) -> StepResult<()> {
    let outcome = engine_connection_state
        .connection_outcome
        .get()
        .ok_or("connection outcome should be set")?;

    let error_path = match &outcome {
        ConnectionOutcome::PermissionDenied(path) | ConnectionOutcome::SocketNotFound(path) => path,
        ConnectionOutcome::OtherError(msg) => {
            return Err(Box::leak(
                format!("expected PermissionDenied or SocketNotFound, got: {msg}").into_boxed_str(),
            ));
        }
        ConnectionOutcome::Success => return Err("expected error but connection succeeded"),
    };

    let socket = engine_connection_state
        .test_socket_path
        .get()
        .ok_or("test socket path should be set")?;

    // Extract path from unix:// URI
    let expected_path = socket.strip_prefix("unix://").unwrap_or(&socket);

    if error_path.contains(expected_path) {
        Ok(())
    } else {
        Err(Box::leak(
            format!("error path '{error_path}' should contain '{expected_path}'").into_boxed_str(),
        ))
    }
}
