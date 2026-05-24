//! Unit tests for the orchestration API module.

use proptest::prelude::*;
use rstest::rstest;
#[cfg(feature = "experimental")]
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{CommandOutcome, RunRequest};
#[cfg(feature = "experimental")]
use super::{list_containers, run_agent, run_token_daemon, stop_container};
#[cfg(feature = "experimental")]
use crate::config::{AppConfig, GitHubConfig};
#[cfg(feature = "experimental")]
use crate::error::{ConfigError, PodbotError};
#[cfg(feature = "experimental")]
use camino::Utf8PathBuf;

mod exec;
#[cfg(feature = "experimental")]
mod log_capture;
#[cfg(feature = "experimental")]
use log_capture::{capture_logs, capture_warning_logs, require_log_contains, require_outcome};

#[cfg(feature = "experimental")]
static CONCURRENT_CREDENTIAL_VALIDATION_CALLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "experimental")]
fn record_concurrent_credential_validation(
    _app_id: u64,
    _private_key_path: &camino::Utf8Path,
) -> super::CredentialValidationFuture<'_> {
    Box::pin(async {
        CONCURRENT_CREDENTIAL_VALIDATION_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
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
fn run_agent_accepts_distinct_run_requests(
    #[case] repository: &str,
    #[case] branch: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = AppConfig::default();
    let request = RunRequest::new(repository, branch)?;

    let mut outcome = None;
    let logs = capture_logs(
        || {
            outcome = Some(run_agent(&config, &request));
        },
        tracing::Level::DEBUG,
    )?;
    let command_outcome = outcome
        .ok_or_else(|| std::io::Error::other("run_agent should have been called"))?
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;

    require_outcome(command_outcome, CommandOutcome::Success)?;
    require_log_contains(&logs, repository, "repository from RunRequest")?;
    require_log_contains(&logs, branch, "branch from RunRequest")?;
    require_log_contains(&logs, "run_agent completed successfully", "success log")?;
    Ok(())
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
fn credential_validation_supports_concurrent_calls_inside_current_runtime() {
    CONCURRENT_CREDENTIAL_VALIDATION_CALLS.store(0, Ordering::SeqCst);
    let runtime = tokio::runtime::Runtime::new().expect("runtime should be created");

    runtime.block_on(async {
        let tasks = (0..4)
            .map(|index| {
                tokio::spawn(async move {
                    let private_key_path = Utf8PathBuf::from(format!("/tmp/test-key-{index}.pem"));
                    super::validate_agent_github_credentials_with(
                        1,
                        &private_key_path,
                        record_concurrent_credential_validation,
                    )
                })
            })
            .collect::<Vec<_>>();

        for task in tasks {
            task.await
                .expect("credential validation task should join")
                .expect("concurrent credential validation should succeed");
        }
    });

    assert_eq!(
        CONCURRENT_CREDENTIAL_VALIDATION_CALLS.load(Ordering::SeqCst),
        4
    );
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
