//! Exec request and client-adapter tests for the API module.
//!
//! This submodule keeps exec-specific request validation, deserialization, and
//! client dispatch fixtures out of the broader API test module. The tests stay
//! close to the API boundary while sharing a focused mock client for the engine
//! port.

use bollard::container::LogOutput;
use futures_util::stream;
use mockall::mock;
use rstest::{fixture, rstest};

use super::super::exec::exec_with_client;
use super::super::{CommandOutcome, ExecMode, ExecRequest};
use crate::engine::{
    ContainerExecClient, CreateExecFuture, ExecMode as EngineExecMode, InspectExecFuture,
    ResizeExecFuture, StartExecFuture,
};
use crate::error::{ConfigError, PodbotError};

mock! {
    #[derive(Debug)]
    ApiExecClient {}

    impl ContainerExecClient for ApiExecClient {
        fn create_exec(&self, container_id: &str, options: bollard::exec::CreateExecOptions<String>) -> CreateExecFuture<'_>;
        fn start_exec(&self, exec_id: &str, options: Option<bollard::exec::StartExecOptions>) -> StartExecFuture<'_>;
        fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
        fn resize_exec(&self, exec_id: &str, options: bollard::exec::ResizeExecOptions) -> ResizeExecFuture<'_>;
    }
}

struct InvalidExecCase {
    container: &'static str,
    command: Vec<String>,
    expected_field: &'static str,
}

type InvalidExecCaseBuilder = fn(&'static str, Vec<String>, &'static str) -> InvalidExecCase;

#[fixture]
fn invalid_exec_case() -> InvalidExecCaseBuilder {
    |container, command, expected_field| InvalidExecCase {
        container,
        command,
        expected_field,
    }
}

#[rstest]
fn exec_request_defaults_to_attached_without_tty() {
    let request =
        ExecRequest::new("sandbox", vec![String::from("echo")]).expect("request should be valid");

    assert_eq!(request.mode(), ExecMode::Attached);
    assert!(!request.tty());
}

#[rstest]
#[case(ExecMode::Attached, EngineExecMode::Attached)]
#[case(ExecMode::Detached, EngineExecMode::Detached)]
#[case(ExecMode::Protocol, EngineExecMode::Protocol)]
fn exec_mode_maps_to_engine_mode(
    #[case] api_mode: ExecMode,
    #[case] expected_engine_mode: EngineExecMode,
) {
    let engine_mode: EngineExecMode = api_mode.into();
    assert_eq!(engine_mode, expected_engine_mode);
}

#[rstest]
fn exec_request_builder_methods_preserve_other_fields() {
    let base = ExecRequest::new("sandbox", vec![String::from("echo"), String::from("hello")])
        .expect("request should be valid");

    let updated = base.clone().with_mode(ExecMode::Protocol).with_tty(true);

    assert_eq!(updated.container(), "sandbox");
    assert_eq!(
        updated.command(),
        &[String::from("echo"), String::from("hello")]
    );
    assert_eq!(updated.mode(), ExecMode::Protocol);
    assert!(!updated.tty());

    assert_eq!(base.container(), "sandbox");
    assert_eq!(
        base.command(),
        &[String::from("echo"), String::from("hello")]
    );
    assert_eq!(base.mode(), ExecMode::Attached);
    assert!(!base.tty());
}

#[rstest]
#[case(ExecMode::Detached)]
#[case(ExecMode::Protocol)]
fn exec_request_normalizes_tty_for_non_attached_modes(#[case] mode: ExecMode) {
    let request = ExecRequest::new("sandbox", vec![String::from("echo")])
        .expect("request should be valid")
        .with_tty(true)
        .with_mode(mode)
        .with_tty(true);

    assert_eq!(request.mode(), mode);
    assert!(
        !request.tty(),
        "tty should be disabled for non-attached modes"
    );
}

#[rstest]
#[case("   ", vec![String::from("echo")], "container")]
#[case("sandbox", Vec::new(), "command")]
#[case("sandbox", vec![String::from("  ")], "command[0]")]
fn exec_request_rejects_missing_required_fields(
    #[case] input_container: &'static str,
    #[case] input_command: Vec<String>,
    #[case] expected_field: &'static str,
    #[from(invalid_exec_case)] build_invalid_exec_case: InvalidExecCaseBuilder,
) {
    let invalid_exec_case = build_invalid_exec_case(input_container, input_command, expected_field);
    let error = ExecRequest::new(invalid_exec_case.container, invalid_exec_case.command)
        .expect_err("invalid exec request should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field })
            if field == invalid_exec_case.expected_field
    ));
}

#[rstest]
#[case::zero_exit_code(0, CommandOutcome::Success)]
#[case::non_zero_exit_code(42, CommandOutcome::CommandExit { code: 42 })]
fn exec_with_client_maps_exit_code_to_outcome(
    #[case] exit_code: i64,
    #[case] expected: CommandOutcome,
) {
    let base_request = ExecRequest::new("sandbox", vec![String::from("echo"), String::from("ok")])
        .expect("request should be valid");
    let request = base_request.with_mode(ExecMode::Detached);
    let runtime = tokio::runtime::Runtime::new().expect("runtime should be created");
    let mut client = MockApiExecClient::new();
    configure_exec_client(&mut client, request.mode(), exit_code);

    let outcome = exec_with_client(&client, runtime.handle(), &request)
        .expect("exit code should map to a command outcome");

    assert_eq!(outcome, expected);
}

#[rstest]
#[case(r#"{"container":"   ","command":["echo"]}"#, "container")]
#[case(r#"{"container":"sandbox","command":[]}"#, "command")]
#[case(r#"{"container":"sandbox","command":["   "]}"#, "command[0]")]
fn exec_request_deserialization_reuses_validation(
    #[case] payload: &str,
    #[case] expected_field: &str,
) {
    let error = serde_json::from_str::<ExecRequest>(payload)
        .expect_err("invalid payload should fail validation");

    assert!(
        error.to_string().contains(expected_field),
        "expected error to mention {expected_field}, got: {error}"
    );
}

#[rstest]
#[case(
    r#"{"container":"sandbox","command":["echo"],"mode":"Detached","tty":true}"#,
    ExecMode::Detached
)]
#[case(
    r#"{"container":"sandbox","command":["echo"],"mode":"Protocol","tty":true}"#,
    ExecMode::Protocol
)]
fn exec_request_deserialization_normalizes_tty_for_non_attached_modes(
    #[case] payload: &str,
    #[case] expected_mode: ExecMode,
) {
    let request = serde_json::from_str::<ExecRequest>(payload).expect("payload should deserialize");

    assert_eq!(request.mode(), expected_mode);
    assert!(
        !request.tty(),
        "tty should be disabled for non-attached modes"
    );
}

fn configure_exec_client(client: &mut MockApiExecClient, mode: ExecMode, exit_code: i64) {
    client.expect_create_exec().times(1).returning(|_, _| {
        Box::pin(async {
            Ok(bollard::exec::CreateExecResults {
                id: String::from("api-exec-id"),
            })
        })
    });

    match mode {
        ExecMode::Attached | ExecMode::Protocol => {
            client.expect_start_exec().times(1).returning(move |_, _| {
                let output_stream = stream::iter(vec![Ok(LogOutput::StdOut {
                    message: Vec::from(&b"api output"[..]).into(),
                })]);
                Box::pin(async move {
                    Ok(bollard::exec::StartExecResults::Attached {
                        output: Box::pin(output_stream),
                        input: Box::pin(tokio::io::sink()),
                    })
                })
            });
        }
        ExecMode::Detached => {
            client.expect_start_exec().times(1).returning(|_, _| {
                Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) })
            });
        }
    }

    match mode {
        ExecMode::Attached => {
            client
                .expect_resize_exec()
                .times(0..)
                .returning(|_, _| Box::pin(async { Ok(()) }));
        }
        ExecMode::Detached | ExecMode::Protocol => {
            client.expect_resize_exec().never();
        }
    }

    client.expect_inspect_exec().times(1).returning(move |_| {
        let inspect = bollard::models::ExecInspectResponse {
            running: Some(false),
            exit_code: Some(exit_code),
            ..bollard::models::ExecInspectResponse::default()
        };
        Box::pin(async move { Ok(inspect) })
    });
}
