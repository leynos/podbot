//! Integration tests proving Podbot can be embedded as a library dependency.
//!
//! These tests exercise the public API surface from a host-application
//! perspective, without importing `podbot::cli` or depending on Clap types
//! directly. This proves that the library boundary is self-contained.

#![allow(
    clippy::too_many_arguments,
    reason = "parameterized rstest functions require multiple test case parameters"
)]

use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, CreateExecResults, StartExecOptions, StartExecResults};
use bollard::models::ExecInspectResponse;
use futures_util::stream;
use mockall::mock;
use rstest::{fixture, rstest};

use podbot::api::{
    CommandOutcome, ExecParams, exec, list_containers, run_agent, run_token_daemon, stop_container,
};
use podbot::config::{AppConfig, CommandIntent, ConfigLoadOptions, ConfigOverrides, load_config};
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, ExecMode, InspectExecFuture, ResizeExecFuture,
    StartExecFuture,
};
use podbot::error::{ConfigError, ContainerError, PodbotError};

mock! {
    #[derive(Debug)]
    EmbedClient {}

    impl ContainerExecClient for EmbedClient {
        fn create_exec(
            &self,
            container_id: &str,
            options: CreateExecOptions<String>,
        ) -> CreateExecFuture<'_>;
        fn start_exec(
            &self,
            exec_id: &str,
            options: Option<StartExecOptions>,
        ) -> StartExecFuture<'_>;
        fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
        fn resize_exec(
            &self,
            exec_id: &str,
            options: bollard::exec::ResizeExecOptions,
        ) -> ResizeExecFuture<'_>;
    }
}

/// Fixture providing a tokio runtime for exec tests.
#[fixture]
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().ok().unwrap_or_else(|| {
        panic!("failed to create tokio runtime");
    })
}

// -------------------------------------------------------------------------
// Configuration loading from host-style call path
// -------------------------------------------------------------------------

#[rstest]
fn load_config_without_cli_types() {
    let options = ConfigLoadOptions {
        config_path_hint: None,
        discover_config: false,
        overrides: ConfigOverrides {
            engine_socket: Some(String::from("unix:///test/embed.sock")),
            image: Some(String::from("test-image:latest")),
            agent_kind: None,
            agent_mode: None,
        },
        command_intent: CommandIntent::Any,
    };

    let config = load_config(&options);
    assert!(config.is_ok(), "config loading should succeed");

    if let Ok(ref cfg) = config {
        assert_eq!(
            cfg.engine_socket.as_deref(),
            Some("unix:///test/embed.sock")
        );
        assert_eq!(cfg.image.as_deref(), Some("test-image:latest"));
    }
}

// -------------------------------------------------------------------------
// Exec orchestration through library API
// -------------------------------------------------------------------------

#[rstest]
#[case::success(
    0,
    ExecMode::Attached,
    vec![String::from("echo"), String::from("hello")],
    |r: &Result<CommandOutcome, PodbotError>| matches!(r, Ok(CommandOutcome::Success)),
    "exec should return Success"
)]
#[case::command_exit(
    42,
    ExecMode::Detached,
    vec![String::from("exit"), String::from("42")],
    |r: &Result<CommandOutcome, PodbotError>| matches!(r, Ok(CommandOutcome::CommandExit { code: 42 })),
    "exec should return CommandExit with code 42"
)]
fn exec_via_library_api_returns_expected_outcome(
    runtime: tokio::runtime::Runtime,
    #[case] exit_code: i64,
    #[case] mode: ExecMode,
    #[case] command: Vec<String>,
    #[case] check: impl Fn(&Result<CommandOutcome, PodbotError>) -> bool,
    #[case] description: &str,
) {
    let mut client = MockEmbedClient::new();
    configure_successful_exec(&mut client, exit_code, mode);

    let result = exec(ExecParams {
        connector: &client,
        container: "embed-sandbox",
        command,
        mode,
        tty: false,
        runtime_handle: runtime.handle(),
    });

    assert!(check(&result), "{description}, got: {result:?}");
}

// -------------------------------------------------------------------------
// Error type contract
// -------------------------------------------------------------------------

#[rstest]
fn exec_failure_returns_container_error(runtime: tokio::runtime::Runtime) {
    let mut client = MockEmbedClient::new();
    client.expect_create_exec().times(1).returning(|_, _| {
        Box::pin(async {
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 500,
                message: String::from("engine unavailable"),
            })
        })
    });

    let result = exec(ExecParams {
        connector: &client,
        container: "embed-sandbox",
        command: vec![String::from("echo"), String::from("fail")],
        mode: ExecMode::Attached,
        tty: false,
        runtime_handle: runtime.handle(),
    });

    assert!(result.is_err(), "exec should return an error");
    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::ExecFailed { .. }))
        ),
        "error should be ContainerError::ExecFailed, got: {result:?}"
    );
}

#[rstest]
fn error_types_are_matchable() {
    let config_err: PodbotError = ConfigError::MissingRequired {
        field: String::from("image"),
    }
    .into();

    assert!(
        matches!(
            config_err,
            PodbotError::Config(ConfigError::MissingRequired { .. })
        ),
        "PodbotError::Config should be matchable"
    );

    let container_err: PodbotError = ContainerError::ConnectionFailed {
        message: String::from("refused"),
    }
    .into();

    assert!(
        matches!(
            container_err,
            PodbotError::Container(ContainerError::ConnectionFailed { .. })
        ),
        "PodbotError::Container should be matchable"
    );
}

// -------------------------------------------------------------------------
// Stub orchestration functions
// -------------------------------------------------------------------------

#[rstest]
fn stub_orchestration_functions_return_success() {
    let config = AppConfig::default();

    assert!(
        matches!(run_agent(&config), Ok(CommandOutcome::Success)),
        "run_agent should return Success"
    );
    assert!(
        matches!(list_containers(), Ok(CommandOutcome::Success)),
        "list_containers should return Success"
    );
    assert!(
        matches!(stop_container("test-ctr"), Ok(CommandOutcome::Success)),
        "stop_container should return Success"
    );
    assert!(
        matches!(run_token_daemon("test-ctr"), Ok(CommandOutcome::Success)),
        "run_token_daemon should return Success"
    );
}

// -------------------------------------------------------------------------
// Test helpers
// -------------------------------------------------------------------------

fn configure_successful_exec(client: &mut MockEmbedClient, exit_code: i64, mode: ExecMode) {
    client.expect_create_exec().times(1).returning(|_, _| {
        Box::pin(async {
            Ok(CreateExecResults {
                id: String::from("embed-exec-id"),
            })
        })
    });

    match mode {
        ExecMode::Attached | ExecMode::Protocol => {
            client.expect_start_exec().times(1).returning(|_, _| {
                let output_stream = stream::iter(vec![Ok(LogOutput::StdOut {
                    message: Vec::from(&b"embed output"[..]).into(),
                })]);
                Box::pin(async move {
                    Ok(StartExecResults::Attached {
                        output: Box::pin(output_stream),
                        input: Box::pin(tokio::io::sink()),
                    })
                })
            });

            client
                .expect_resize_exec()
                .times(0..)
                .returning(|_, _| Box::pin(async { Ok(()) }));
        }
        ExecMode::Detached => {
            client
                .expect_start_exec()
                .times(1)
                .returning(|_, _| Box::pin(async { Ok(StartExecResults::Detached) }));

            client.expect_resize_exec().never();
        }
        _ => panic!("unexpected exec mode in test setup"),
    }

    client.expect_inspect_exec().times(1).returning(move |_| {
        let inspect = ExecInspectResponse {
            running: Some(false),
            exit_code: Some(exit_code),
            ..ExecInspectResponse::default()
        };
        Box::pin(async move { Ok(inspect) })
    });
}
