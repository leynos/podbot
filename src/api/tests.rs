//! Unit tests for the orchestration API module.

use bollard::container::LogOutput;
use camino::Utf8PathBuf;
use futures_util::stream;
use mockall::mock;
use rstest::rstest;

use super::{
    CommandOutcome, ExecMode, ExecRequest, exec_with_client, list_containers, run_agent,
    run_token_daemon, stop_container,
};
use crate::config::{AppConfig, GitHubConfig};
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

#[rstest]
fn command_outcome_success_equals_itself() {
    assert_eq!(CommandOutcome::Success, CommandOutcome::Success);
}

#[rstest]
fn command_outcome_exit_preserves_code() {
    let outcome = CommandOutcome::CommandExit { code: 42 };
    assert_eq!(outcome, CommandOutcome::CommandExit { code: 42 });
}

#[rstest]
fn command_outcome_success_differs_from_exit_zero() {
    assert_ne!(
        CommandOutcome::Success,
        CommandOutcome::CommandExit { code: 0 }
    );
}

#[rstest]
fn command_outcome_is_copy() {
    let outcome = CommandOutcome::CommandExit { code: 7 };
    let copied = outcome;
    assert_eq!(outcome, copied);
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
fn exec_request_rejects_blank_container() {
    let error = ExecRequest::new("   ", vec![String::from("echo")])
        .expect_err("blank container should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field }) if field == "container"
    ));
}

#[rstest]
fn exec_request_rejects_empty_command() {
    let error =
        ExecRequest::new("sandbox", Vec::new()).expect_err("empty command should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field }) if field == "command"
    ));
}

#[rstest]
fn exec_request_rejects_blank_executable() {
    let error = ExecRequest::new("sandbox", vec![String::from("  ")])
        .expect_err("blank executable should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field }) if field == "command[0]"
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
fn run_agent_requires_complete_github_config() {
    let config = AppConfig {
        github: GitHubConfig {
            app_id: Some(1),
            installation_id: None,
            private_key_path: Some(Utf8PathBuf::from("/tmp/test-key.pem")),
        },
        ..AppConfig::default()
    };

    let error = run_agent(&config).expect_err("incomplete GitHub config should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field })
            if field.contains("github.installation_id")
    ));
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

#[rstest]
#[case::run_agent("run_agent")]
#[case::list_containers("list_containers")]
#[case::stop_container("stop_container")]
#[case::run_token_daemon("run_token_daemon")]
fn stub_returns_success(#[case] stub: &str) {
    let config = AppConfig::default();
    let outcome = match stub {
        "run_agent" => run_agent(&config),
        "list_containers" => list_containers(),
        "stop_container" => stop_container("test-container"),
        "run_token_daemon" => run_token_daemon("test-container-id"),
        other => panic!("unknown stub: {other}"),
    }
    .expect("stub should return Ok");
    assert_eq!(outcome, CommandOutcome::Success);
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
