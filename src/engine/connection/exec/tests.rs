//! Unit tests for container exec lifecycle handling.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::stream;
use mockall::mock;
use rstest::{fixture, rstest};

use super::terminal::TerminalSize;
use super::*;
use crate::error::{ConfigError, ContainerError, PodbotError};

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

#[fixture]
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("runtime creation should succeed")
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

fn setup_inspect_exec_completion(client: &mut MockExecClient, exit_code: i64) {
    client.expect_inspect_exec().times(1).returning(move |_| {
        Box::pin(async move {
            Ok(bollard::models::ExecInspectResponse {
                running: Some(false),
                exit_code: Some(exit_code),
                ..bollard::models::ExecInspectResponse::default()
            })
        })
    });
}

fn assert_exec_request_validation_error(
    result: Result<ExecRequest, PodbotError>,
    expected_field: &str,
) {
    let field = match result {
        Err(PodbotError::Config(
            ConfigError::MissingRequired { field } | ConfigError::InvalidValue { field, .. },
        )) => field,
        other => panic!("expected validation error for '{expected_field}', got {other:?}"),
    };
    assert_eq!(
        field, expected_field,
        "expected validation error for '{expected_field}', got field '{field}'"
    );
}

#[rstest]
fn exec_request_rejects_empty_command() {
    let result = ExecRequest::new("sandbox", vec![], ExecMode::Attached);
    assert_exec_request_validation_error(result, "command");
}

#[rstest]
#[case(vec![String::new()])]
#[case(vec![String::from("   "), String::from("echo")])]
fn exec_request_rejects_blank_executable_entry(#[case] command: Vec<String>) {
    let result = ExecRequest::new("sandbox", command, ExecMode::Attached);
    assert!(
        matches!(
            result,
            Err(PodbotError::Config(ConfigError::InvalidValue { ref field, .. }))
                if field == "command"
        ),
        "expected invalid executable error, got {result:?}"
    );
}

#[rstest]
#[case(vec![String::from("echo"), String::new()])]
#[case(vec![String::from("echo"), String::from("   ")])]
fn exec_request_allows_blank_non_executable_entries(#[case] command: Vec<String>) {
    let expected = command.clone();
    let request = ExecRequest::new("sandbox", command, ExecMode::Attached)
        .expect("command with blank non-executable arguments should be accepted");
    assert_eq!(request.command(), expected.as_slice());
}

#[rstest]
fn exec_request_rejects_blank_container_id() {
    let result = ExecRequest::new("   ", vec![String::from("echo")], ExecMode::Detached);
    assert_exec_request_validation_error(result, "container");
}

#[rstest]
fn exec_async_detached_returns_exit_code(runtime: tokio::runtime::Runtime) {
    let mut client = MockExecClient::new();
    client
        .expect_create_exec()
        .times(1)
        .returning(|container_id, options| {
            assert_eq!(container_id, "sandbox-123");
            assert_eq!(options.cmd, Some(vec![String::from("true")]));
            Box::pin(async {
                Ok(CreateExecResults {
                    id: String::from("exec-1"),
                })
            })
        });
    client
        .expect_start_exec()
        .times(1)
        .returning(|exec_id, options| {
            assert_eq!(exec_id, "exec-1");
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

    let inspect_call_count = Arc::new(AtomicUsize::new(0));
    let inspect_call_count_for_mock = Arc::clone(&inspect_call_count);
    client
        .expect_inspect_exec()
        .times(2)
        .returning(move |exec_id| {
            assert_eq!(exec_id, "exec-1");
            let call_index = inspect_call_count_for_mock.fetch_add(1, Ordering::SeqCst);
            let response = if call_index == 0 {
                bollard::models::ExecInspectResponse {
                    running: Some(true),
                    exit_code: None,
                    ..bollard::models::ExecInspectResponse::default()
                }
            } else {
                bollard::models::ExecInspectResponse {
                    running: Some(false),
                    exit_code: Some(7),
                    ..bollard::models::ExecInspectResponse::default()
                }
            };
            Box::pin(async move { Ok(response) })
        });

    let request = ExecRequest::new(
        "sandbox-123",
        vec![String::from("true")],
        ExecMode::Detached,
    )
    .expect("detached request should build");

    let result = runtime
        .block_on(EngineConnector::exec_async(&client, &request))
        .expect("exec should succeed");

    assert_eq!(result.exec_id(), "exec-1");
    assert_eq!(result.exit_code(), 7);
}

#[rstest]
fn exec_async_maps_create_exec_failures(runtime: tokio::runtime::Runtime) {
    let mut client = MockExecClient::new();
    client
        .expect_create_exec()
        .times(1)
        .returning(|_, _| Box::pin(async { Err(BollardError::RequestTimeoutError) }));

    let request = ExecRequest::new(
        "sandbox-123",
        vec![String::from("false")],
        ExecMode::Detached,
    )
    .expect("detached request should build");

    let result = runtime.block_on(EngineConnector::exec_async(&client, &request));
    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::ExecFailed { ref container_id, ref message }))
                if container_id == "sandbox-123" && message.contains("create exec failed")
        ),
        "expected create-exec failure mapping, got {result:?}"
    );
}

#[rstest]
fn exec_async_errors_when_exit_code_missing(runtime: tokio::runtime::Runtime) {
    let mut client = MockExecClient::new();
    client.expect_create_exec().times(1).returning(|_, _| {
        Box::pin(async {
            Ok(CreateExecResults {
                id: String::from("exec-2"),
            })
        })
    });
    client
        .expect_start_exec()
        .times(1)
        .returning(|_, _| Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) }));
    client.expect_inspect_exec().times(1).returning(|_| {
        Box::pin(async {
            Ok(bollard::models::ExecInspectResponse {
                running: Some(false),
                exit_code: None,
                ..bollard::models::ExecInspectResponse::default()
            })
        })
    });

    let request = ExecRequest::new(
        "sandbox-123",
        vec![String::from("false")],
        ExecMode::Detached,
    )
    .expect("detached request should build");

    let result = runtime.block_on(EngineConnector::exec_async(&client, &request));
    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::ExecFailed { ref message, .. }))
                if message.contains("without an exit code")
        ),
        "expected missing-exit-code failure, got {result:?}"
    );
}

#[rstest]
fn exec_async_attached_rejects_detached_start_response(runtime: tokio::runtime::Runtime) {
    let mut client = MockExecClient::new();
    client.expect_create_exec().times(1).returning(|_, _| {
        Box::pin(async {
            Ok(CreateExecResults {
                id: String::from("exec-3"),
            })
        })
    });
    client
        .expect_start_exec()
        .times(1)
        .returning(|_, _| Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) }));

    let request = ExecRequest::new(
        "sandbox-123",
        vec![String::from("echo"), String::from("hello")],
        ExecMode::Attached,
    )
    .expect("attached request should build");

    let result = runtime.block_on(EngineConnector::exec_async(&client, &request));
    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::ExecFailed { ref message, .. }))
                if message.contains("detached start result")
        ),
        "expected attached/detached mismatch failure, got {result:?}"
    );
}

#[rstest]
fn exec_async_attached_calls_resize_when_tty_enabled(runtime: tokio::runtime::Runtime) {
    let mut client = MockExecClient::new();
    setup_create_exec_expectation(&mut client, "exec-4", true);
    setup_start_exec_attached(&mut client, true, vec![&b"ok"[..]]);
    setup_resize_exec_expectation(&mut client, "exec-4", 120, 42);
    setup_inspect_exec_completion(&mut client, 0);

    let request = ExecRequest::new(
        "sandbox-123",
        vec![String::from("echo"), String::from("hello")],
        ExecMode::Attached,
    )
    .expect("attached request should build");
    let terminal_size_provider = StubTerminalSizeProvider {
        terminal_size: Some(TerminalSize {
            width: 120,
            height: 42,
        }),
    };

    let result = runtime.block_on(EngineConnector::exec_async_with_terminal_size_provider(
        &client,
        &request,
        &terminal_size_provider,
    ));
    assert!(
        result.is_ok(),
        "attached execution should succeed: {result:?}"
    );
}

#[rstest]
fn exec_async_attached_propagates_resize_failures(runtime: tokio::runtime::Runtime) {
    let mut client = MockExecClient::new();
    client.expect_create_exec().times(1).returning(|_, _| {
        Box::pin(async {
            Ok(CreateExecResults {
                id: String::from("exec-6"),
            })
        })
    });
    client.expect_start_exec().times(1).returning(|_, options| {
        assert_eq!(
            options,
            Some(StartExecOptions {
                detach: false,
                tty: true,
                output_capacity: None
            })
        );
        let output_stream = stream::iter(Vec::<Result<LogOutput, BollardError>>::new());
        Box::pin(async move {
            Ok(bollard::exec::StartExecResults::Attached {
                output: Box::pin(output_stream),
                input: Box::pin(tokio::io::sink()),
            })
        })
    });
    client
        .expect_resize_exec()
        .times(1)
        .returning(|_, _| Box::pin(async { Err(BollardError::RequestTimeoutError) }));
    client.expect_inspect_exec().never();

    let request = ExecRequest::new(
        "sandbox-123",
        vec![String::from("echo"), String::from("hello")],
        ExecMode::Attached,
    )
    .expect("attached request should build");
    let terminal_size_provider = StubTerminalSizeProvider {
        terminal_size: Some(TerminalSize {
            width: 120,
            height: 42,
        }),
    };

    let result = runtime.block_on(EngineConnector::exec_async_with_terminal_size_provider(
        &client,
        &request,
        &terminal_size_provider,
    ));
    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::ExecFailed { ref message, .. }))
                if message.contains("resize exec failed")
        ),
        "expected resize failure mapping, got {result:?}"
    );
}

#[rstest]
fn exec_async_attached_skips_resize_when_tty_disabled(runtime: tokio::runtime::Runtime) {
    let mut client = MockExecClient::new();
    setup_create_exec_expectation(&mut client, "exec-5", false);
    setup_start_exec_attached(&mut client, false, vec![]);
    client.expect_resize_exec().never();
    setup_inspect_exec_completion(&mut client, 0);

    let request = ExecRequest::new(
        "sandbox-123",
        vec![String::from("echo"), String::from("hello")],
        ExecMode::Attached,
    )
    .expect("attached request should build")
    .with_tty(false);
    let terminal_size_provider = StubTerminalSizeProvider {
        terminal_size: Some(TerminalSize {
            width: 80,
            height: 24,
        }),
    };

    let result = runtime.block_on(EngineConnector::exec_async_with_terminal_size_provider(
        &client,
        &request,
        &terminal_size_provider,
    ));
    assert!(
        result.is_ok(),
        "attached execution should succeed: {result:?}"
    );
}
