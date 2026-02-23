//! Shared helpers for detached exec assertions in unit tests.

use super::*;

/// Expected detached-exec outcome used by unit-test assertions.
///
/// `exec_id` is the daemon-assigned exec session identifier that should be
/// returned by the connector. `exit_code` is the final process exit status
/// expected from inspect polling.
#[derive(Clone, Copy)]
pub(super) struct DetachedExecExpectation<'a> {
    pub(super) exec_id: &'a str,
    pub(super) exit_code: i64,
}

/// Configure a mock client to accept detached start options and return a
/// detached start response.
///
/// The helper asserts that `start_exec` receives `detach = true` and
/// `tty = false`, then returns `StartExecResults::Detached`.
pub(super) fn setup_start_exec_detached(client: &mut MockExecClient) {
    client.expect_start_exec().times(1).returning(|_, options| {
        assert_eq!(
            options,
            Some(StartExecOptions {
                detach: true,
                tty: false,
                output_capacity: None
            })
        );
        Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) })
    });
}

/// Configure a mock client to simulate an attached-request mismatch by
/// returning a detached start response.
///
/// The helper asserts that `start_exec` was called with attached options
/// (`detach = false`, `tty = true`) before returning `Detached`.
pub(super) fn setup_start_exec_returns_detached(client: &mut MockExecClient) {
    client.expect_start_exec().times(1).returning(|_, options| {
        assert_eq!(
            options,
            Some(StartExecOptions {
                detach: false,
                tty: true,
                output_capacity: None
            })
        );
        Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) })
    });
}

/// Execute a detached request using the supplied runtime and mock client.
///
/// Returns the connector's `ExecResult` on success, or propagates the mapped
/// `PodbotError` when execution fails.
pub(super) fn execute_detached_and_assert_result(
    runtime: &tokio::runtime::Runtime,
    client: &MockExecClient,
    request: &ExecRequest,
) -> Result<ExecResult, PodbotError> {
    runtime.block_on(EngineConnector::exec_async(client, request))
}

/// Assert that a detached execution result matches the expected id and exit
/// code.
///
/// `result` is the successful execution output to validate, and `expected`
/// provides the identifier and exit code that should be present.
pub(super) fn assert_detached_exec_expectation(
    result: &ExecResult,
    expected: DetachedExecExpectation<'_>,
) {
    assert_eq!(result.exec_id(), expected.exec_id);
    assert_eq!(result.exit_code(), expected.exit_code);
}

/// Assert that execution failed with an `ExecFailed` message containing a
/// required fragment.
///
/// Panics with `assertion_context` when the result is not the expected error
/// shape or when the message does not contain `expected_message_fragment`.
pub(super) fn assert_exec_failed_with_message(
    result: Result<ExecResult, PodbotError>,
    expected_message_fragment: &str,
    assertion_context: &str,
) {
    match result {
        Err(PodbotError::Container(ContainerError::ExecFailed { message, .. }))
            if message.contains(expected_message_fragment) => {}
        other => panic!("{assertion_context}, got {other:?}"),
    }
}

/// Assert that execution failed for a specific container and message
/// fragment.
///
/// Panics with `assertion_context` when the result does not match
/// `ContainerError::ExecFailed` for `expected_container_id`, or when the
/// message does not contain `expected_message_fragment`.
pub(super) fn assert_exec_failed_for_container_with_message(
    result: Result<ExecResult, PodbotError>,
    expected_container_id: &str,
    expected_message_fragment: &str,
    assertion_context: &str,
) {
    match result {
        Err(PodbotError::Container(ContainerError::ExecFailed {
            container_id,
            message,
        })) if container_id == expected_container_id
            && message.contains(expected_message_fragment) => {}
        other => panic!("{assertion_context}, got {other:?}"),
    }
}
