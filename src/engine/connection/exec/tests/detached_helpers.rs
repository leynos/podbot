//! Shared helpers for detached exec assertions in unit tests.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct DetachedExecExpectation<'a> {
    pub(super) exec_id: &'a str,
    pub(super) exit_code: i64,
}

pub(super) fn execute_detached_and_assert_result_impl(
    runtime: &tokio::runtime::Runtime,
    client: &MockExecClient,
    request: &ExecRequest,
    expected: DetachedExecExpectation<'_>,
) {
    let result = runtime
        .block_on(EngineConnector::exec_async(client, request))
        .expect("exec should succeed");
    assert_eq!(result.exec_id(), expected.exec_id);
    assert_eq!(result.exit_code(), expected.exit_code);
}

pub(super) fn assert_exec_failed_with_message_impl(
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

pub(super) fn assert_exec_failed_for_container_with_message_impl(
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
