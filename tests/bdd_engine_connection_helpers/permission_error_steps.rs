//! Permission error step definitions for BDD tests.
//!
//! This module contains step definitions for testing socket permission
//! and not-found error handling scenarios.

use std::path::Path;

use podbot::engine::EngineConnector;
use podbot::error::{ContainerError, PodbotError};
use rstest_bdd_macros::{given, then, when};

use super::{ConnectionOutcome, EngineConnectionState, StepResult};

// =============================================================================
// Given step definitions
// =============================================================================

/// Returns a platform-appropriate socket path that commonly requires elevated
/// permissions.
fn restricted_socket_path() -> String {
    #[cfg(unix)]
    {
        String::from("unix:///var/run/docker.sock")
    }
    #[cfg(windows)]
    {
        String::from("npipe:////./pipe/docker_engine")
    }
    #[cfg(not(any(unix, windows)))]
    {
        String::from("unix:///var/run/docker.sock")
    }
}

/// Returns a platform-appropriate socket path that should not exist.
fn missing_socket_path() -> String {
    #[cfg(unix)]
    {
        String::from("unix:///nonexistent/podbot-test-socket.sock")
    }
    #[cfg(windows)]
    {
        String::from("npipe:////./pipe/nonexistent-podbot-test-socket")
    }
    #[cfg(not(any(unix, windows)))]
    {
        String::from("unix:///nonexistent/podbot-test-socket.sock")
    }
}

/// Extracts a path component from a socket URI for comparison in assertions.
fn socket_path_component(socket_uri: &str) -> &Path {
    let path = socket_uri
        .strip_prefix("unix://")
        .or_else(|| socket_uri.strip_prefix("npipe://"))
        .unwrap_or(socket_uri);
    Path::new(path)
}

/// Returns true if an error message includes a hint and at least one command.
fn has_remediation_guidance(message: &str) -> bool {
    message.contains("Hint:")
        && (message.contains("sudo usermod -aG docker")
            || message.contains("sudo systemctl start docker")
            || message.contains("systemctl --user start podman.socket")
            || message.contains("/run/user/$UID/podman/podman.sock"))
}

/// Set up a socket path that typically requires elevated permissions.
///
/// Uses the system `Docker` socket which normally requires docker group
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
        .set(restricted_socket_path());
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
        .set(missing_socket_path());
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
            let message = ContainerError::PermissionDenied { path: path.clone() }.to_string();
            ConnectionOutcome::PermissionDenied {
                path: path.display().to_string(),
                message,
            }
        }
        Err(PodbotError::Container(ContainerError::SocketNotFound { path })) => {
            let message = ContainerError::SocketNotFound { path: path.clone() }.to_string();
            ConnectionOutcome::SocketNotFound {
                path: path.display().to_string(),
                message,
            }
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
        ConnectionOutcome::PermissionDenied { .. } => Ok(()),
        ConnectionOutcome::Success => {
            rstest_bdd::skip!("user has permission to access the socket");
        }
        ConnectionOutcome::SocketNotFound { .. } => {
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
        ConnectionOutcome::SocketNotFound { .. } => Ok(()),
        ConnectionOutcome::OtherError(msg) => Err(Box::leak(
            format!("expected SocketNotFound, got: {msg}").into_boxed_str(),
        )),
        ConnectionOutcome::Success => Err("expected error but connection succeeded"),
        ConnectionOutcome::PermissionDenied { path, .. } => Err(Box::leak(
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

    let (error_path, message) = match &outcome {
        ConnectionOutcome::PermissionDenied { path, message }
        | ConnectionOutcome::SocketNotFound { path, message } => (path, message),
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

    let expected_path = socket_path_component(&socket).display().to_string();

    if !error_path.contains(&expected_path) {
        Err(Box::leak(
            format!("error path '{error_path}' should contain '{expected_path}'").into_boxed_str(),
        ))
    } else if !has_remediation_guidance(message) {
        Err(Box::leak(
            format!("error message should include actionable guidance, got: {message}")
                .into_boxed_str(),
        ))
    } else {
        Ok(())
    }
}
