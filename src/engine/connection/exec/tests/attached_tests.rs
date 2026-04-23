//! Attached-session exec lifecycle tests.

use super::*;

fn assert_attached_success_exit_code(result: &ExecResult) {
    assert_eq!(result.exit_code(), 0);
}

struct AttachedResizeCase {
    tty: bool,
    exec_id: &'static str,
    terminal_size: TerminalSize,
    output_messages: Vec<&'static [u8]>,
    should_resize: bool,
}

#[rstest]
#[serial]
#[case(AttachedResizeCase {
    tty: true,
    exec_id: "exec-4",
    terminal_size: TerminalSize {
        width: 120,
        height: 42,
    },
    output_messages: vec![&b"ok"[..]],
    should_resize: true,
})]
#[case(AttachedResizeCase {
    tty: false,
    exec_id: "exec-5",
    terminal_size: TerminalSize {
        width: 80,
        height: 24,
    },
    output_messages: vec![],
    should_resize: false,
})]
fn exec_async_attached_resize_behaviour(
    runtime: RuntimeFixture,
    #[case] case: AttachedResizeCase,
) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    setup_create_exec_expectation(&mut client, case.exec_id, case.tty);
    setup_start_exec_attached(&mut client, case.tty, case.output_messages);
    setup_attached_resize_expectation_for_case(
        &mut client,
        case.exec_id,
        case.terminal_size,
        case.should_resize,
    );
    setup_inspect_exec_once(&mut client, Some(0));

    let request = make_attached_exec_request("sandbox-123", case.tty)?;
    let terminal_size_provider = StubTerminalSizeProvider {
        terminal_size: Some(case.terminal_size),
    };
    let result =
        execute_and_assert_success(&runtime_handle, &client, &request, &terminal_size_provider)?;
    assert_attached_success_exit_code(&result);
    Ok(())
}

#[rstest]
#[serial]
fn exec_async_attached_propagates_resize_failures(runtime: RuntimeFixture) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    setup_create_exec_expectation(&mut client, "exec-6", true);
    setup_start_exec_attached(&mut client, true, vec![]);
    setup_resize_exec_failure(&mut client, bollard::errors::Error::RequestTimeoutError);
    client.expect_inspect_exec().never();

    let request = make_attached_exec_request("sandbox-123", true)?;
    let terminal_size_provider = make_terminal_size_provider(120, 42);

    let result = runtime_handle.block_on(EngineConnector::exec_async_with_terminal_size_provider(
        &client,
        &request,
        &terminal_size_provider,
    ));
    assert_exec_failed_with_message(
        result,
        "resize exec failed",
        "expected resize failure mapping",
    );
    Ok(())
}
