//! TCP connection step definitions for BDD tests.
//!
//! This module contains step definitions for testing TCP endpoint
//! connection behaviour, verifying that TCP connections are created
//! lazily and that errors are classified as generic connection failures
//! rather than socket-specific errors.

use podbot::engine::EngineConnector;
use podbot::error::{ContainerError, PodbotError};
use rstest_bdd_macros::{given, then, when};

use super::{ConnectionOutcome, EngineConnectionState, StepResult};

// =============================================================================
// Given step definitions
// =============================================================================

/// Configure a TCP endpoint for connection testing.
///
/// TCP connections via Bollard's `connect_with_http` are lazy and do
/// not validate connectivity at construction time.
#[given("a TCP endpoint is configured")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
pub fn tcp_endpoint_is_configured(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    engine_connection_state
        .test_socket_path
        .set(String::from("tcp://localhost:2375"));
    Ok(())
}

/// Configure a TCP endpoint that will fail during health check.
///
/// Uses a documentation-reserved IP address (RFC 5737, 192.0.2.0/24)
/// to ensure connection failure during the health check phase without
/// risk of accidentally connecting to a real service.
#[given("a TCP endpoint that will fail health check")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult for consistency"
)]
pub fn tcp_endpoint_will_fail_health_check(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    engine_connection_state
        .test_socket_path
        .set(String::from("tcp://192.0.2.1:2375"));
    Ok(())
}

// =============================================================================
// When step definitions
// =============================================================================

/// Attempt a TCP connection (connect only, no health check).
///
/// TCP connections are lazy: `connect()` creates the client configuration
/// synchronously without reaching out to the remote host. This step should
/// always produce a `Success` outcome.
#[when("a TCP connection is attempted")]
pub fn tcp_connection_is_attempted(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let socket = engine_connection_state
        .test_socket_path
        .get()
        .ok_or("test socket path should be set")?;

    let result = EngineConnector::connect(&socket);

    let outcome = match result {
        Ok(_) => ConnectionOutcome::Success,
        Err(e) => ConnectionOutcome::OtherError(e.to_string()),
    };

    engine_connection_state.connection_outcome.set(outcome);
    Ok(())
}

/// Attempt a TCP connection with health check verification.
///
/// The connect phase will succeed (lazy), but the health check ping
/// will fail because no daemon is listening at the endpoint. This
/// tests that TCP errors are classified as generic `ConnectionFailed`
/// rather than socket-specific errors.
#[when("a TCP connection with health check is attempted")]
pub fn tcp_connection_with_health_check(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let socket = engine_connection_state
        .test_socket_path
        .get()
        .ok_or("test socket path should be set")?;

    let rt = tokio::runtime::Runtime::new().map_err(|e| {
        Box::leak(format!("failed to create tokio runtime: {e}").into_boxed_str()) as &'static str
    })?;
    let result = rt.block_on(EngineConnector::connect_and_verify_async(&socket));

    let outcome = match result {
        Ok(_) => ConnectionOutcome::Success,
        Err(PodbotError::Container(
            ref container_err @ ContainerError::PermissionDenied { ref path },
        )) => ConnectionOutcome::PermissionDenied {
            path: path.display().to_string(),
            message: container_err.to_string(),
        },
        Err(PodbotError::Container(
            ref container_err @ ContainerError::SocketNotFound { ref path },
        )) => ConnectionOutcome::SocketNotFound {
            path: path.display().to_string(),
            message: container_err.to_string(),
        },
        Err(e) => ConnectionOutcome::OtherError(e.to_string()),
    };

    engine_connection_state.connection_outcome.set(outcome);
    Ok(())
}

// =============================================================================
// Then step definitions
// =============================================================================

/// Assert that the TCP connection client was created successfully.
///
/// This verifies the lazy connection behaviour: TCP endpoints create
/// the Bollard client without attempting to reach the remote host.
#[then("the connection client is created successfully")]
pub fn connection_client_created(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let outcome = engine_connection_state
        .connection_outcome
        .get()
        .ok_or("connection outcome should be set")?;

    match outcome {
        ConnectionOutcome::Success => Ok(()),
        ConnectionOutcome::OtherError(msg) => Err(Box::leak(
            format!("expected successful connection, got error: {msg}").into_boxed_str(),
        )),
        _ => Err("expected successful connection"),
    }
}

/// Assert that a generic connection failure error was returned.
///
/// TCP endpoints should never produce `SocketNotFound` or
/// `PermissionDenied` because there is no filesystem path to
/// attribute the error to. All TCP errors must map to
/// `ConnectionFailed` or health check variants.
#[then("a connection failure error is returned")]
pub fn connection_failure_error_returned(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let outcome = engine_connection_state
        .connection_outcome
        .get()
        .ok_or("connection outcome should be set")?;

    match outcome {
        ConnectionOutcome::OtherError(_) => Ok(()),
        ConnectionOutcome::Success => {
            rstest_bdd::skip!(
                "TCP endpoint unexpectedly responded; daemon may be running at configured address"
            );
        }
        ConnectionOutcome::SocketNotFound { path, .. } => Err(Box::leak(
            format!("TCP should not produce SocketNotFound, but got SocketNotFound for: {path}")
                .into_boxed_str(),
        )),
        ConnectionOutcome::PermissionDenied { path, .. } => Err(Box::leak(
            format!(
                "TCP should not produce PermissionDenied, but got PermissionDenied for: {path}"
            )
            .into_boxed_str(),
        )),
    }
}
