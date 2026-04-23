//! Detached-session exec lifecycle tests.

use super::*;

#[rstest]
fn exec_async_detached_returns_exit_code(runtime: RuntimeFixture) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    setup_create_exec_simple(&mut client, "exec-1");
    setup_start_exec_detached(&mut client);
    lifecycle_helpers::setup_inspect_exec_with_running_transition(&mut client, 7, 1);

    let request = make_detached_exec_request("sandbox-123", vec![String::from("true")])?;
    let expected = DetachedExecExpectation {
        exec_id: "exec-1",
        exit_code: 7,
    };
    let result = execute_detached_and_assert_result(&runtime_handle, &client, &request)?;
    assert_detached_exec_expectation(&result, expected);
    Ok(())
}
