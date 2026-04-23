//! Internal-feature integration suite for embedding and compatibility paths.
//!
//! These tests run only with `feature = "internal"` because they exercise
//! internal shims such as `podbot::engine`. The stable public embedding
//! boundary remains `podbot::api`, `podbot::config`, and `podbot::error`;
//! `podbot::engine` and `podbot::github` are internal compatibility modules,
//! while `podbot::cli` visibility is controlled by the `cli` feature.

#![cfg(feature = "internal")]

mod test_utils;

use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, CreateExecResults, StartExecOptions, StartExecResults};
use bollard::models::ExecInspectResponse;
use futures_util::stream;
use mockall::mock;
use rstest::{fixture, rstest};

use podbot::api::{CommandOutcome, ExecMode, ExecRequest};
#[cfg(feature = "experimental")]
use podbot::api::{list_containers, run_agent, run_token_daemon, stop_container};
#[cfg(feature = "experimental")]
use podbot::config::AppConfig;
use podbot::config::{CommandIntent, ConfigLoadOptions, ConfigOverrides, load_config};
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture,
};
use podbot::error::{ConfigError, ContainerError, PodbotError};
use test_utils::exec_outcome_with_client;

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
fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Runtime::new()
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

struct LibraryApiExecTestCase {
    exit_code: i64,
    mode: ExecMode,
    command: Vec<String>,
    check: fn(&Result<CommandOutcome, PodbotError>) -> bool,
    description: &'static str,
}

#[rstest]
#[case::success(LibraryApiExecTestCase {
    exit_code: 0,
    mode: ExecMode::Attached,
    command: vec![String::from("echo"), String::from("hello")],
    check: |r| matches!(r, Ok(CommandOutcome::Success)),
    description: "exec should return Success",
})]
#[case::command_exit(LibraryApiExecTestCase {
    exit_code: 42,
    mode: ExecMode::Detached,
    command: vec![String::from("exit"), String::from("42")],
    check: |r| matches!(r, Ok(CommandOutcome::CommandExit { code: 42 })),
    description: "exec should return CommandExit with code 42",
})]
fn exec_via_library_api_returns_expected_outcome(
    runtime: Result<tokio::runtime::Runtime, std::io::Error>,
    #[case] test_case: LibraryApiExecTestCase,
) -> Result<(), Box<dyn std::error::Error>> {
    let rt = runtime?;
    let mut client = MockEmbedClient::new();
    configure_successful_exec(&mut client, test_case.exit_code, test_case.mode);

    let request = ExecRequest::new("embed-sandbox", test_case.command)?
        .with_mode(test_case.mode)
        .with_tty(false);
    let result = exec_outcome_with_client(&client, rt.handle(), &request);

    assert_exec_outcome_matches(&result, test_case.check, test_case.description);
    Ok(())
}

// -------------------------------------------------------------------------
// Error type contract
// -------------------------------------------------------------------------

#[rstest]
#[case::create(FailAt::Create)]
#[case::start(FailAt::Start)]
#[case::inspect(FailAt::Inspect)]
#[case::missing_exit_code(FailAt::InspectMissingExitCode)]
fn exec_failure_returns_container_error(
    runtime: Result<tokio::runtime::Runtime, std::io::Error>,
    #[case] fail_at: FailAt,
) -> Result<(), Box<dyn std::error::Error>> {
    let rt = runtime?;
    let mut client = MockEmbedClient::new();
    let mode = configure_failing_exec(&mut client, fail_at);

    let request = ExecRequest::new(
        "embed-sandbox",
        vec![String::from("echo"), String::from("fail")],
    )?
    .with_mode(mode)
    .with_tty(false);
    let result = exec_outcome_with_client(&client, rt.handle(), &request);

    assert_exec_failed_with_container_error(&result, fail_at);
    Ok(())
}

// Note: resize_exec failure testing is omitted from library boundary tests because:
// 1. resize_exec is only called when both tty=true AND stdio is a terminal, which requires
//    complex mocking of the terminal size provider infrastructure.
// 2. This failure path is already comprehensively tested in the unit tests at
//    src/engine/connection/exec/tests.rs (see setup_resize_exec_failure test helper).
// 3. The library boundary tests focus on API-level error propagation, and resize failures
//    propagate the same ContainerError::ExecFailed variant as other exec failures.

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
#[cfg(feature = "experimental")]
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

#[derive(Debug, Clone, Copy)]
enum FailAt {
    Create,
    Start,
    Inspect,
    InspectMissingExitCode,
}

fn expect_create_exec_ok(client: &mut MockEmbedClient) {
    client.expect_create_exec().times(1).returning(|_, _| {
        Box::pin(async {
            Ok(CreateExecResults {
                id: String::from("exec-id"),
            })
        })
    });
}

fn expect_start_exec_detached_ok(client: &mut MockEmbedClient) {
    client
        .expect_start_exec()
        .times(1)
        .returning(|_, _| Box::pin(async { Ok(StartExecResults::Detached) }));
}

fn configure_failing_exec(client: &mut MockEmbedClient, fail_at: FailAt) -> ExecMode {
    match fail_at {
        FailAt::Create => {
            client.expect_create_exec().times(1).returning(|_, _| {
                Box::pin(async {
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 500,
                        message: String::from("engine unavailable"),
                    })
                })
            });
            ExecMode::Attached
        }
        FailAt::Start => {
            expect_create_exec_ok(client);
            client.expect_start_exec().times(1).returning(|_, _| {
                Box::pin(async {
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 500,
                        message: String::from("failed to start exec"),
                    })
                })
            });
            ExecMode::Attached
        }
        FailAt::Inspect => {
            expect_create_exec_ok(client);
            expect_start_exec_detached_ok(client);
            client.expect_inspect_exec().times(1).returning(|_| {
                Box::pin(async {
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 500,
                        message: String::from("failed to inspect exec"),
                    })
                })
            });
            ExecMode::Detached
        }
        FailAt::InspectMissingExitCode => {
            expect_create_exec_ok(client);
            expect_start_exec_detached_ok(client);
            client.expect_inspect_exec().times(1).returning(|_| {
                let inspect = ExecInspectResponse {
                    running: Some(false),
                    exit_code: None,
                    ..ExecInspectResponse::default()
                };
                Box::pin(async move { Ok(inspect) })
            });
            ExecMode::Detached
        }
    }
}

fn assert_exec_outcome_matches(
    result: &Result<CommandOutcome, PodbotError>,
    check: fn(&Result<CommandOutcome, PodbotError>) -> bool,
    description: &str,
) {
    assert!(check(result), "{description}, got: {result:?}");
}

fn assert_exec_failed_with_container_error(
    result: &Result<CommandOutcome, PodbotError>,
    fail_at: FailAt,
) {
    assert!(result.is_err(), "exec should return an error");
    assert!(
        matches!(
            result,
            Err(PodbotError::Container(ContainerError::ExecFailed { .. }))
        ),
        "error should be ContainerError::ExecFailed for {fail_at:?}, got: {result:?}"
    );
}

fn configure_successful_exec(client: &mut MockEmbedClient, exit_code: i64, mode: ExecMode) {
    client.expect_create_exec().times(1).returning(|_, _| {
        Box::pin(async {
            Ok(CreateExecResults {
                id: String::from("embed-exec-id"),
            })
        })
    });

    // ExecMode is marked #[non_exhaustive], so we must handle the wildcard pattern.
    // If new variants are added, this match will need updating.
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

            client.expect_resize_exec().never();
        }
        ExecMode::Detached => {
            client
                .expect_start_exec()
                .times(1)
                .returning(|_, _| Box::pin(async { Ok(StartExecResults::Detached) }));

            client.expect_resize_exec().never();
        }
        _ => {
            // Fallback for future ExecMode variants that haven't been explicitly handled.
            // Tests using unsupported modes will fail with a clear error message.
            panic!(
                "configure_successful_exec does not support ExecMode::{mode:?}. \
                 Please add explicit handling for this variant."
            );
        }
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
