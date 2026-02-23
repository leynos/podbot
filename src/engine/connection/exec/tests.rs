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

#[rstest]
fn exec_request_rejects_empty_command() {
    let result = ExecRequest::new("sandbox", vec![], ExecMode::Attached);
    assert!(
        matches!(
            result,
            Err(PodbotError::Config(ConfigError::MissingRequired { ref field }))
                if field == "command"
        ),
        "expected missing command error, got {result:?}"
    );
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
    assert!(
        matches!(
            result,
            Err(PodbotError::Config(ConfigError::MissingRequired { ref field }))
                if field == "container"
        ),
        "expected missing container error, got {result:?}"
    );
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
    client
        .expect_create_exec()
        .times(1)
        .returning(|_, options| {
            assert_eq!(options.tty, Some(true));
            Box::pin(async {
                Ok(CreateExecResults {
                    id: String::from("exec-4"),
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
        let output_stream = stream::iter(vec![Ok(LogOutput::StdOut {
            message: Vec::from(&b"ok"[..]).into(),
        })]);
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
        .returning(|exec_id, options| {
            assert_eq!(exec_id, "exec-4");
            assert_eq!(
                options,
                ResizeExecOptions {
                    width: 120,
                    height: 42
                }
            );
            Box::pin(async { Ok(()) })
        });
    client.expect_inspect_exec().times(1).returning(|_| {
        Box::pin(async {
            Ok(bollard::models::ExecInspectResponse {
                running: Some(false),
                exit_code: Some(0),
                ..bollard::models::ExecInspectResponse::default()
            })
        })
    });

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
    client
        .expect_create_exec()
        .times(1)
        .returning(|_, options| {
            assert_eq!(options.tty, Some(false));
            Box::pin(async {
                Ok(CreateExecResults {
                    id: String::from("exec-5"),
                })
            })
        });
    client.expect_start_exec().times(1).returning(|_, options| {
        assert_eq!(
            options,
            Some(StartExecOptions {
                detach: false,
                tty: false,
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
    client.expect_resize_exec().never();
    client.expect_inspect_exec().times(1).returning(|_| {
        Box::pin(async {
            Ok(bollard::models::ExecInspectResponse {
                running: Some(false),
                exit_code: Some(0),
                ..bollard::models::ExecInspectResponse::default()
            })
        })
    });

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
