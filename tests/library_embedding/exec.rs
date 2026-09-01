//! Exec-focused support and boundary tests for library embedding.
//!
//! This module owns the mock container-exec client and the tests that use it,
//! keeping the parent embedding suite below the repository file-size limit.

use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, CreateExecResults, StartExecOptions, StartExecResults};
use bollard::models::ExecInspectResponse;
use futures_util::stream;
use mockall::mock;
use podbot::api::{CommandOutcome, ExecMode, ExecRequest};
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture,
};
use podbot::error::{ContainerError, PodbotError};
use rstest::{fixture, rstest};

use super::test_utils::exec_outcome_with_client;

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

/// Asserts that an exec result satisfies `check`, reporting `description` on
/// failure.
///
/// A macro rather than a function so the panic is raised inside the calling
/// test, keeping failure line numbers at the call site.
macro_rules! assert_exec_outcome_matches {
    ($result:expr, $check:expr, $description:expr $(,)?) => {{
        let result = $result;
        assert!(($check)(&result), "{}, got: {result:?}", $description);
    }};
}

/// Asserts that an exec result failed with `ContainerError::ExecFailed` for the
/// container under test.
///
/// A macro rather than a function so the panic is raised inside the calling
/// test; `exec_failure` must be in scope at the call site.
macro_rules! assert_exec_failed_with_container_error {
    ($result:expr, $fail_at:expr $(,)?) => {{
        let fail_at = $fail_at;
        let failure = exec_failure($result).unwrap_or_else(|actual| {
            panic!("error should be ContainerError::ExecFailed for {fail_at:?}, got: {actual}")
        });

        assert_eq!(
            failure.container_id, "embed-sandbox",
            "exec failure for {fail_at:?} should name the container under test"
        );
        assert!(
            !failure.message.is_empty(),
            "exec failure for {fail_at:?} should carry a message"
        );
    }};
}

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
) {
    let rt = runtime.expect("tokio runtime should be created");
    let mut client = MockEmbedClient::new();
    configure_successful_exec(&mut client, test_case.exit_code, test_case.mode)
        .expect("mock exec expectations should be configurable for the requested mode");

    let request = ExecRequest::new("embed-sandbox", test_case.command)
        .expect("exec request should be valid")
        .with_mode(test_case.mode)
        .with_tty(false);
    let result = exec_outcome_with_client(&client, rt.handle(), &request);

    assert_exec_outcome_matches!(result, test_case.check, test_case.description);
}

#[derive(Debug, Clone, Copy)]
enum FailAt {
    Create,
    Start,
    Inspect,
    InspectMissingExitCode,
}

#[rstest]
#[case::create(FailAt::Create)]
#[case::start(FailAt::Start)]
#[case::inspect(FailAt::Inspect)]
#[case::missing_exit_code(FailAt::InspectMissingExitCode)]
fn exec_failure_returns_container_error(
    runtime: Result<tokio::runtime::Runtime, std::io::Error>,
    #[case] fail_at: FailAt,
) {
    let rt = runtime.expect("tokio runtime should be created");
    let mut client = MockEmbedClient::new();
    let mode = configure_failing_exec(&mut client, fail_at);

    let request = ExecRequest::new(
        "embed-sandbox",
        vec![String::from("echo"), String::from("fail")],
    )
    .expect("exec request should be valid")
    .with_mode(mode)
    .with_tty(false);
    let result = exec_outcome_with_client(&client, rt.handle(), &request);

    assert_exec_failed_with_container_error!(result, fail_at);
}

// Resize failures need a terminal-size provider, and the unit suite already
// verifies that path. These boundary tests cover the shared `ExecFailed`
// result exposed to library embedders instead.

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

/// The payload of a `ContainerError::ExecFailed` error, extracted for
/// assertion.
#[derive(Debug)]
struct ExecFailure {
    container_id: String,
    message: String,
}

/// Extracts the `ExecFailed` payload from an exec result.
///
/// Returns the debug rendering of the actual result in the error case so the
/// caller can report the mismatch; this helper never panics.
fn exec_failure(result: Result<CommandOutcome, PodbotError>) -> Result<ExecFailure, String> {
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

/// Configures the mock client for a successful exec in `mode`.
///
/// `ExecMode` is `#[non_exhaustive]`, so unsupported future variants are
/// reported as an error for the calling test to surface rather than panicking
/// here.
fn configure_successful_exec(
    client: &mut MockEmbedClient,
    exit_code: i64,
    mode: ExecMode,
) -> Result<(), String> {
    client.expect_create_exec().times(1).returning(|_, _| {
        Box::pin(async {
            Ok(CreateExecResults {
                id: String::from("embed-exec-id"),
            })
        })
    });

    // ExecMode is marked #[non_exhaustive], so the wildcard arm must remain.
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
            return Err(format!(
                concat!(
                    "configure_successful_exec does not support ExecMode::{mode:?}. ",
                    "Please add explicit handling for this variant."
                ),
                mode = mode,
            ));
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

    Ok(())
}
