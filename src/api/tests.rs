//! Unit tests for the orchestration API module.

use bollard::container::LogOutput;
use futures_util::stream;
use mockall::mock;
use proptest::prelude::*;
use rstest::rstest;
#[cfg(feature = "experimental")]
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use super::exec::exec_with_client;
use super::{CommandOutcome, ExecMode, ExecRequest, RunRequest};
#[cfg(feature = "experimental")]
use super::{list_containers, run_agent, run_token_daemon, stop_container};
#[cfg(feature = "experimental")]
use crate::config::{AppConfig, GitHubConfig};
use crate::engine::{
    ContainerExecClient, CreateExecFuture, ExecMode as EngineExecMode, InspectExecFuture,
    ResizeExecFuture, StartExecFuture,
};
use crate::error::{ConfigError, PodbotError};
#[cfg(feature = "experimental")]
use camino::Utf8PathBuf;

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
fn run_request_preserves_repository_and_branch() {
    let request =
        RunRequest::new("owner/name", "main").expect("valid run request should be created");

    assert_eq!(request.repository(), "owner/name");
    assert_eq!(request.branch(), "main");
}

#[rstest]
#[case::empty_repository("", "main", "run.repository")]
#[case::blank_repository("   ", "main", "run.repository")]
#[case::empty_branch("owner/name", "", "run.branch")]
#[case::blank_branch("owner/name", "   ", "run.branch")]
fn run_request_rejects_empty_values(
    #[case] repository: &str,
    #[case] branch: &str,
    #[case] expected_field: &str,
) {
    let error =
        RunRequest::new(repository, branch).expect_err("empty request values should be rejected");

    assert!(
        error.to_string().contains(expected_field),
        "expected error to mention {expected_field}, got {error}"
    );
}

proptest! {
    #[test]
    fn run_request_validation_follows_trim_semantics(
        repository in "[\\sA-Za-z0-9_/.-]{0,64}",
        branch in "[\\sA-Za-z0-9_/.-]{0,64}",
    ) {
        let result = RunRequest::new(repository.clone(), branch.clone());

        if repository.trim().is_empty() || branch.trim().is_empty() {
            prop_assert!(result.is_err());
        } else {
            prop_assert!(result.is_ok());
            let request = result.unwrap_or_else(|error| panic!("valid request rejected: {error}"));
            prop_assert_eq!(request.repository(), repository);
            prop_assert_eq!(request.branch(), branch);
        }
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
#[cfg(feature = "experimental")]
fn run_agent_requires_complete_github_config() {
    let config = AppConfig {
        github: GitHubConfig {
            app_id: Some(1),
            installation_id: None,
            private_key_path: Some(Utf8PathBuf::from("/tmp/test-key.pem")),
        },
        ..AppConfig::default()
    };

    let request = RunRequest::new("owner/name", "main").expect("request should be valid");

    let error =
        run_agent(&config, &request).expect_err("incomplete GitHub config should be rejected");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::MissingRequired { field })
            if field.contains("github.installation_id")
    ));
}

#[rstest]
#[cfg(feature = "experimental")]
#[case::feature_branch("owner/feature", "feature/run-request")]
#[case::release_branch("team/service", "release-2026")]
fn run_agent_accepts_distinct_run_requests(#[case] repository: &str, #[case] branch: &str) {
    let config = AppConfig::default();
    let request = RunRequest::new(repository, branch).expect("request should be valid");

    let outcome = run_agent(&config, &request).expect("valid run request should succeed");

    assert_eq!(outcome, CommandOutcome::Success);
}

#[rstest]
#[cfg(feature = "experimental")]
fn run_agent_logs_request_context_when_github_config_validation_fails()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = AppConfig {
        github: GitHubConfig {
            app_id: Some(1),
            installation_id: None,
            private_key_path: Some(Utf8PathBuf::from("/tmp/test-key.pem")),
        },
        ..AppConfig::default()
    };
    let request =
        RunRequest::new("owner/request-context", "feature/log-context").expect("request is valid");

    let logs = capture_warning_logs(|| {
        let _result = run_agent(&config, &request);
    })?;

    require_log_contains(
        &logs,
        "GitHub configuration validation failed for run request",
        "configuration validation message",
    )?;
    require_log_contains(&logs, "owner/request-context", "repository from RunRequest")?;
    require_log_contains(&logs, "feature/log-context", "branch from RunRequest")?;
    Ok(())
}

#[rstest]
#[cfg(feature = "experimental")]
fn warn_github_validation_failed_logs_credential_message_and_request_context()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let request =
        RunRequest::new("owner/auth-context", "feature/auth-log").expect("request is valid");
    let error = PodbotError::from(crate::error::GitHubError::AuthenticationFailed {
        message: String::from("test authentication failure"),
    });

    let logs = capture_warning_logs(|| {
        super::warn_github_validation_failed(
            &request,
            &error,
            Some("42"),
            "GitHub credential authentication failed for run request",
        );
    })?;

    require_log_contains(
        &logs,
        "GitHub credential authentication failed for run request",
        "credential authentication message",
    )?;
    require_log_contains(&logs, "owner/auth-context", "repository from RunRequest")?;
    require_log_contains(&logs, "feature/auth-log", "branch from RunRequest")?;
    require_log_contains(&logs, "42", "app id context")?;
    Ok(())
}

#[rstest]
#[cfg(feature = "experimental")]
fn credential_validation_thread_panic_maps_to_github_error() {
    let error = super::credential_validation_thread_panicked();

    assert!(matches!(
        error,
        PodbotError::GitHub(crate::error::GitHubError::AuthenticationFailed { message })
            if message == "GitHub credential validation thread panicked"
    ));
}

#[cfg(feature = "experimental")]
#[derive(Clone)]
struct SharedLogWriter {
    output: Arc<Mutex<Vec<u8>>>,
}

#[cfg(feature = "experimental")]
struct SharedLogBuffer {
    output: Arc<Mutex<Vec<u8>>>,
}

#[cfg(feature = "experimental")]
impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for SharedLogWriter {
    type Writer = SharedLogBuffer;

    fn make_writer(&'writer self) -> Self::Writer {
        SharedLogBuffer {
            output: Arc::clone(&self.output),
        }
    }
}

#[cfg(feature = "experimental")]
impl std::io::Write for SharedLogBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut output = self
            .output
            .lock()
            .map_err(|_| std::io::Error::other("log buffer mutex poisoned"))?;
        output.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(feature = "experimental")]
fn capture_warning_logs(
    operation: impl FnOnce(),
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let output = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedLogWriter {
        output: Arc::clone(&output),
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::WARN)
        .finish();

    tracing::subscriber::with_default(subscriber, operation);

    let bytes = output
        .lock()
        .map_err(|error| {
            Box::new(std::io::Error::other(format!(
                "log buffer mutex poisoned: {error}"
            ))) as Box<dyn std::error::Error + Send + Sync>
        })?
        .clone();
    String::from_utf8(bytes)
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
}

#[cfg(feature = "experimental")]
fn require_log_contains(
    logs: &str,
    expected: &str,
    description: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if logs.contains(expected) {
        Ok(())
    } else {
        Err(Box::new(std::io::Error::other(format!(
            "warning should include {description}: {logs}"
        ))))
    }
}

#[rstest]
#[cfg(feature = "experimental")]
fn credential_validation_uses_local_runtime_without_current_handle() {
    let calls = AtomicUsize::new(0);
    let private_key_path = Utf8PathBuf::from("/tmp/test-key.pem");

    super::validate_agent_github_credentials_with(1, &private_key_path, |_, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    })
    .expect("local runtime credential validation should succeed");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[rstest]
#[cfg(feature = "experimental")]
fn credential_validation_uses_scoped_thread_inside_current_runtime() {
    let calls = AtomicUsize::new(0);
    let private_key_path = Utf8PathBuf::from("/tmp/test-key.pem");
    let runtime = tokio::runtime::Runtime::new().expect("runtime should be created");

    runtime
        .block_on(async {
            super::validate_agent_github_credentials_with(1, &private_key_path, |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            })
        })
        .expect("scoped-thread credential validation should succeed");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[rstest]
#[cfg(feature = "experimental")]
fn credential_validation_scoped_thread_panic_maps_to_github_error() {
    let private_key_path = Utf8PathBuf::from("/tmp/test-key.pem");
    let runtime = tokio::runtime::Runtime::new().expect("runtime should be created");

    let error = runtime
        .block_on(async {
            super::validate_agent_github_credentials_with(1, &private_key_path, |_, _| {
                Box::pin(async { panic!("credential validation panic") })
            })
        })
        .expect_err("scoped-thread panic should be mapped to an error");

    assert!(matches!(
        error,
        PodbotError::GitHub(crate::error::GitHubError::AuthenticationFailed { message })
            if message == "GitHub credential validation thread panicked"
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
#[cfg(feature = "experimental")]
fn stub_returns_success(#[case] stub: &str) {
    let config = AppConfig::default();
    let request = RunRequest::new("owner/name", "main").expect("request should be valid");
    let outcome = match stub {
        "run_agent" => run_agent(&config, &request),
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
