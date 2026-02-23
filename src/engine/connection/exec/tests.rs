//! Unit tests for container exec lifecycle handling.

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::stream;
use mockall::mock;
use rstest::{fixture, rstest};

use super::terminal::TerminalSize;
use super::*;
use crate::error::{ContainerError, PodbotError};
mod detached_helpers;
mod lifecycle_helpers;
mod validation_tests;

mock! {
    #[derive(Debug)]
    ExecClient {}

    impl ContainerExecClient for ExecClient {
        fn create_exec(&self, container_id: &str, options: CreateExecOptions<String>) -> CreateExecFuture<'_>;
        fn start_exec(&self, exec_id: &str, options: Option<StartExecOptions>) -> StartExecFuture<'_>;
        fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
        fn resize_exec(&self, exec_id: &str, options: ResizeExecOptions) -> ResizeExecFuture<'_>;
    }
}

struct StubTerminalSizeProvider {
    terminal_size: Option<TerminalSize>,
}

impl TerminalSizeProvider for StubTerminalSizeProvider {
    fn terminal_size(&self) -> Option<TerminalSize> {
        self.terminal_size
    }
}

type RuntimeFixture = std::io::Result<tokio::runtime::Runtime>;
type TestResult = std::io::Result<()>;

struct AttachedResizeCase {
    tty: bool,
    exec_id: &'static str,
    terminal_size: TerminalSize,
    output_messages: Vec<&'static [u8]>,
    should_resize: bool,
}

#[fixture]
fn runtime() -> RuntimeFixture {
    tokio::runtime::Runtime::new()
}

fn setup_create_exec_expectation(client: &mut MockExecClient, exec_id: &'static str, tty: bool) {
    client
        .expect_create_exec()
        .times(1)
        .returning(move |_, options| {
            assert_eq!(options.tty, Some(tty));
            Box::pin(async move {
                Ok(CreateExecResults {
                    id: String::from(exec_id),
                })
            })
        });
}

fn setup_start_exec_attached(
    client: &mut MockExecClient,
    tty: bool,
    output_messages: Vec<&'static [u8]>,
) {
    client
        .expect_start_exec()
        .times(1)
        .returning(move |_, options| {
            assert_eq!(
                options,
                Some(StartExecOptions {
                    detach: false,
                    tty,
                    output_capacity: None
                })
            );
            let output_chunks = output_messages
                .iter()
                .map(|message| {
                    Ok(LogOutput::StdOut {
                        message: Vec::from(*message).into(),
                    })
                })
                .collect::<Vec<Result<LogOutput, BollardError>>>();
            let output_stream = stream::iter(output_chunks);
            Box::pin(async move {
                Ok(bollard::exec::StartExecResults::Attached {
                    output: Box::pin(output_stream),
                    input: Box::pin(tokio::io::sink()),
                })
            })
        });
}

fn setup_resize_exec_expectation(
    client: &mut MockExecClient,
    exec_id: &'static str,
    width: u16,
    height: u16,
) {
    client
        .expect_resize_exec()
        .times(1)
        .returning(move |actual_exec_id, options| {
            assert_eq!(actual_exec_id, exec_id);
            assert_eq!(options, ResizeExecOptions { width, height });
            Box::pin(async { Ok(()) })
        });
}

fn setup_resize_exec_failure(client: &mut MockExecClient, error: BollardError) {
    client
        .expect_resize_exec()
        .times(1)
        .return_once(move |_, _| Box::pin(async move { Err(error) }));
}

fn setup_inspect_exec_once(client: &mut MockExecClient, exit_code: Option<i64>) {
    client.expect_inspect_exec().times(1).returning(move |_| {
        Box::pin(async move {
            Ok(bollard::models::ExecInspectResponse {
                running: Some(false),
                exit_code,
                ..bollard::models::ExecInspectResponse::default()
            })
        })
    });
}

fn setup_start_exec_detached(client: &mut MockExecClient) {
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

fn setup_start_exec_returns_detached(client: &mut MockExecClient) {
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

fn setup_create_exec_failure(client: &mut MockExecClient, error: BollardError) {
    client
        .expect_create_exec()
        .times(1)
        .return_once(move |_, _| Box::pin(async move { Err(error) }));
}

fn setup_create_exec_simple(client: &mut MockExecClient, exec_id: &'static str) {
    client.expect_create_exec().times(1).returning(move |_, _| {
        Box::pin(async move {
            Ok(CreateExecResults {
                id: String::from(exec_id),
            })
        })
    });
}

fn make_attached_exec_request(container_id: &str, tty: bool) -> ExecRequest {
    ExecRequest::new(
        container_id,
        vec![String::from("echo"), String::from("hello")],
        ExecMode::Attached,
    )
    .expect("attached request should build")
    .with_tty(tty)
}

fn make_terminal_size_provider(width: u16, height: u16) -> StubTerminalSizeProvider {
    StubTerminalSizeProvider {
        terminal_size: Some(TerminalSize { width, height }),
    }
}

fn execute_and_assert_success(
    runtime: &tokio::runtime::Runtime,
    client: &MockExecClient,
    request: &ExecRequest,
    terminal_size_provider: &StubTerminalSizeProvider,
) {
    let result = runtime.block_on(EngineConnector::exec_async_with_terminal_size_provider(
        client,
        request,
        terminal_size_provider,
    ));
    assert!(
        result.is_ok(),
        "attached execution should succeed: {result:?}"
    );
}

fn setup_attached_resize_expectation_for_case(
    client: &mut MockExecClient,
    exec_id: &'static str,
    terminal_size: TerminalSize,
    should_resize: bool,
) {
    if should_resize {
        setup_resize_exec_expectation(client, exec_id, terminal_size.width, terminal_size.height);
    } else {
        client.expect_resize_exec().never();
    }
}

fn make_detached_exec_request(container_id: &str, command: Vec<String>) -> ExecRequest {
    ExecRequest::new(container_id, command, ExecMode::Detached)
        .expect("detached request should build")
}

use detached_helpers::{
    DetachedExecExpectation,
    assert_exec_failed_for_container_with_message_impl as assert_exec_failed_for_container_with_message,
    assert_exec_failed_with_message_impl as assert_exec_failed_with_message,
    execute_detached_and_assert_result_impl as execute_detached_and_assert_result,
};

#[rstest]
fn exec_async_detached_returns_exit_code(runtime: RuntimeFixture) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    setup_create_exec_simple(&mut client, "exec-1");
    setup_start_exec_detached(&mut client);
    lifecycle_helpers::setup_inspect_exec_with_running_transition(&mut client, 7, 1);

    let request = make_detached_exec_request("sandbox-123", vec![String::from("true")]);
    execute_detached_and_assert_result(
        &runtime_handle,
        &client,
        &request,
        DetachedExecExpectation {
            exec_id: "exec-1",
            exit_code: 7,
        },
    );
    Ok(())
}

#[rstest]
fn exec_async_maps_create_exec_failures(runtime: RuntimeFixture) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    setup_create_exec_failure(&mut client, BollardError::RequestTimeoutError);

    let request = make_detached_exec_request("sandbox-123", vec![String::from("false")]);

    let result = runtime_handle.block_on(EngineConnector::exec_async(&client, &request));
    assert_exec_failed_for_container_with_message(
        result,
        "sandbox-123",
        "create exec failed",
        "expected create-exec failure mapping",
    );
    Ok(())
}

#[rstest]
fn exec_async_errors_when_exit_code_missing(runtime: RuntimeFixture) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    setup_create_exec_simple(&mut client, "exec-2");
    setup_start_exec_detached(&mut client);
    setup_inspect_exec_once(&mut client, None);

    let request = make_detached_exec_request("sandbox-123", vec![String::from("false")]);

    let result = runtime_handle.block_on(EngineConnector::exec_async(&client, &request));
    assert_exec_failed_with_message(
        result,
        "without an exit code",
        "expected missing-exit-code failure",
    );
    Ok(())
}

#[rstest]
fn exec_async_attached_rejects_detached_start_response(runtime: RuntimeFixture) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    setup_create_exec_simple(&mut client, "exec-3");
    setup_start_exec_returns_detached(&mut client);

    let request = ExecRequest::new(
        "sandbox-123",
        vec![String::from("echo"), String::from("hello")],
        ExecMode::Attached,
    )
    .expect("attached request should build");

    let result = runtime_handle.block_on(EngineConnector::exec_async(&client, &request));
    assert_exec_failed_with_message(
        result,
        "detached start result",
        "expected attached/detached mismatch failure",
    );
    Ok(())
}

#[rstest]
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

    let request = make_attached_exec_request("sandbox-123", case.tty);
    let terminal_size_provider = StubTerminalSizeProvider {
        terminal_size: Some(case.terminal_size),
    };
    execute_and_assert_success(&runtime_handle, &client, &request, &terminal_size_provider);
    Ok(())
}

#[rstest]
fn exec_async_attached_propagates_resize_failures(runtime: RuntimeFixture) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    setup_create_exec_expectation(&mut client, "exec-6", true);
    setup_start_exec_attached(&mut client, true, vec![]);
    setup_resize_exec_failure(&mut client, BollardError::RequestTimeoutError);
    client.expect_inspect_exec().never();

    let request = make_attached_exec_request("sandbox-123", true);
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
