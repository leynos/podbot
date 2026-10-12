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

/// The payload of a `ContainerError::ExecFailed` error, extracted for
/// assertion.
#[derive(Debug)]
pub(super) struct ExecFailure {
    pub(super) container_id: String,
    pub(super) message: String,
}

/// Extracts the `ExecFailed` payload from an execution result.
///
/// Returns the debug rendering of the actual result in the error case so the
/// caller can report the mismatch; this helper never panics.
///
/// # Examples
///
/// ```ignore
/// let failure = exec_failure(result).expect("expected an ExecFailed error");
/// assert!(failure.message.contains("resize exec failed"));
/// ```
pub(super) fn exec_failure(result: Result<ExecResult, PodbotError>) -> Result<ExecFailure, String> {
    match result {
        Err(PodbotError::Container(ContainerError::ExecFailed {
            container_id,
            message,
        })) => Ok(ExecFailure {
            container_id,
            message,
        }),
        other => Err(format!("{other:?}")),
    }
}

/// Asserts that an exec result failed with an `ExecFailed` error whose message
/// contains `$fragment`, optionally for a specific container.
///
/// A macro rather than a function so failures report the calling test's line
/// number. `exec_failure` must be in scope at the call site, which the
/// `tests::helpers` re-export provides.
///
/// # Examples
///
/// ```ignore
/// assert_exec_failed!(result, "resize exec failed", "expected resize mapping");
/// assert_exec_failed!(result, container: "sandbox-123", "detached start result", context);
/// ```
macro_rules! assert_exec_failed {
    ($result:expr, $fragment:expr, $context:expr $(,)?) => {
        match exec_failure($result) {
            Ok(failure) => assert!(
                failure.message.contains($fragment),
                "{}: expected message containing '{}', got '{}'",
                $context,
                $fragment,
                failure.message
            ),
            Err(actual) => panic!("{}, got {}", $context, actual),
        }
    };
    ($result:expr, container: $container_id:expr, $fragment:expr, $context:expr $(,)?) => {
        match exec_failure($result) {
            Ok(failure) => {
                assert_eq!(
                    failure.container_id, $container_id,
                    "{}: unexpected container id",
                    $context
                );
                assert!(
                    failure.message.contains($fragment),
                    "{}: expected message containing '{}', got '{}'",
                    $context,
                    $fragment,
                    failure.message
                );
            }
            Err(actual) => panic!("{}, got {}", $context, actual),
        }
    };
}

pub(super) use assert_exec_failed;
