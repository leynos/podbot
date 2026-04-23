//! Shared fixtures and helper functions for exec lifecycle unit tests.

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::stream;
use mockall::mock;
use rstest::fixture;

use super::super::terminal::TerminalSize;
use super::super::*;
use crate::error::PodbotError;

pub(super) use super::detached_helpers::{
    DetachedExecExpectation, assert_detached_exec_expectation,
    assert_exec_failed_for_container_with_message, assert_exec_failed_with_message,
    execute_detached_and_assert_result, setup_start_exec_detached,
    setup_start_exec_returns_detached,
};

mock! {
    #[derive(Debug)]
    pub(super) ExecClient {}

    impl ContainerExecClient for ExecClient {
        fn create_exec(&self, container_id: &str, options: CreateExecOptions<String>) -> CreateExecFuture<'_>;
        fn start_exec(&self, exec_id: &str, options: Option<StartExecOptions>) -> StartExecFuture<'_>;
        fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
        fn resize_exec(&self, exec_id: &str, options: ResizeExecOptions) -> ResizeExecFuture<'_>;
    }
}

pub(super) struct StubTerminalSizeProvider {
    pub(super) terminal_size: Option<TerminalSize>,
}

impl TerminalSizeProvider for StubTerminalSizeProvider {
    fn terminal_size(&self) -> Option<TerminalSize> {
        self.terminal_size
    }
}

pub(super) type RuntimeFixture = std::io::Result<tokio::runtime::Runtime>;
pub(super) type TestResult = Result<(), Box<dyn std::error::Error>>;

#[fixture]
pub(super) fn runtime() -> RuntimeFixture {
    tokio::runtime::Runtime::new()
}

pub(super) fn setup_create_exec_expectation(
    client: &mut MockExecClient,
    exec_id: &'static str,
    tty: bool,
) {
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

pub(super) fn setup_start_exec_attached(
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

pub(super) fn setup_resize_exec_expectation(
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

pub(super) fn setup_resize_exec_failure(client: &mut MockExecClient, error: BollardError) {
    client
        .expect_resize_exec()
        .times(1)
        .return_once(move |_, _| Box::pin(async move { Err(error) }));
}

pub(super) fn setup_inspect_exec_once(client: &mut MockExecClient, exit_code: Option<i64>) {
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

pub(super) fn setup_create_exec_failure(client: &mut MockExecClient, error: BollardError) {
    client
        .expect_create_exec()
        .times(1)
        .return_once(move |_, _| Box::pin(async move { Err(error) }));
}

pub(super) fn setup_create_exec_simple(client: &mut MockExecClient, exec_id: &'static str) {
    client.expect_create_exec().times(1).returning(move |_, _| {
        Box::pin(async move {
            Ok(CreateExecResults {
                id: String::from(exec_id),
            })
        })
    });
}

pub(super) fn setup_create_exec_failure_scenario(client: &mut MockExecClient) {
    setup_create_exec_failure(client, BollardError::RequestTimeoutError);
}

pub(super) fn setup_missing_exit_code_scenario(client: &mut MockExecClient) {
    setup_create_exec_simple(client, "exec-2");
    setup_start_exec_detached(client);
    setup_inspect_exec_once(client, None);
}

pub(super) fn setup_attached_detached_response_scenario(client: &mut MockExecClient) {
    setup_create_exec_simple(client, "exec-3");
    setup_start_exec_returns_detached(client);
}

pub(super) fn make_attached_exec_request(
    container_id: &str,
    tty: bool,
) -> Result<ExecRequest, PodbotError> {
    Ok(ExecRequest::new(
        container_id,
        vec![String::from("echo"), String::from("hello")],
        ExecMode::Attached,
    )?
    .with_tty(tty))
}

pub(super) fn make_terminal_size_provider(width: u16, height: u16) -> StubTerminalSizeProvider {
    StubTerminalSizeProvider {
        terminal_size: Some(TerminalSize { width, height }),
    }
}

pub(super) fn execute_and_assert_success(
    runtime: &tokio::runtime::Runtime,
    client: &MockExecClient,
    request: &ExecRequest,
    terminal_size_provider: &StubTerminalSizeProvider,
) -> Result<ExecResult, PodbotError> {
    runtime.block_on(EngineConnector::exec_async_with_terminal_size_provider(
        client,
        request,
        terminal_size_provider,
    ))
}

pub(super) fn setup_attached_resize_expectation_for_case(
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

pub(super) fn make_detached_exec_request(
    container_id: &str,
    command: Vec<String>,
) -> Result<ExecRequest, PodbotError> {
    ExecRequest::new(container_id, command, ExecMode::Detached)
}
