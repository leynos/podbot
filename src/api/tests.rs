//! Unit tests for the orchestration API module.

use proptest::prelude::*;
use rstest::rstest;
#[cfg(feature = "experimental")]
use std::sync::Arc;
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

    #[test]
    #[cfg(feature = "experimental")]
    fn run_agent_accepts_owner_name_repository_format(
        owner in "[A-Za-z0-9._-]{1,32}",
        name in "[A-Za-z0-9._-]{1,32}",
        branch in "[A-Za-z0-9._/-]{1,32}",
    ) {
        let repository = format!("{owner}/{name}");
        let request = RunRequest::new(repository, branch)
            .unwrap_or_else(|error| panic!("valid request should be constructed: {error}"));

        let outcome = run_agent(&AppConfig::default(), &request)
            .unwrap_or_else(|error| panic!("valid request should be accepted: {error}"));

        prop_assert_eq!(outcome, CommandOutcome::Success);
    }

    #[test]
    #[cfg(feature = "experimental")]
    fn run_agent_rejects_repository_segments_containing_whitespace(
        prefix in "[A-Za-z0-9._-]{0,16}",
        whitespace in "\\s+",
        suffix in "[A-Za-z0-9._-]{0,16}",
        valid_segment in "[A-Za-z0-9._-]{1,32}",
        whitespace_in_owner in any::<bool>(),
    ) {
        let invalid_segment = format!("{prefix}{whitespace}{suffix}");
        prop_assume!(!invalid_segment.trim().is_empty());
        let repository = if whitespace_in_owner {
            format!("{invalid_segment}/{valid_segment}")
        } else {
            format!("{valid_segment}/{invalid_segment}")
        };
        let request = RunRequest::new(repository, "main")
            .unwrap_or_else(|error| panic!("non-empty request should be constructed: {error}"));

        let error = run_agent(&AppConfig::default(), &request)
            .expect_err("repository whitespace should be rejected");

        let is_repository_error = matches!(
            error,
            PodbotError::Config(ConfigError::InvalidValue { field, .. })
                if field == "run.repository"
        );
        prop_assert!(is_repository_error);
    }

    #[test]
    #[cfg(feature = "experimental")]
    fn run_agent_rejects_branch_whitespace(
        owner in "[A-Za-z0-9._-]{1,32}",
        name in "[A-Za-z0-9._-]{1,32}",
        prefix in "[A-Za-z0-9._/-]{0,16}",
        whitespace in "\\s+",
        suffix in "[A-Za-z0-9._/-]{0,16}",
    ) {
        let repository = format!("{owner}/{name}");
        let branch = format!("{prefix}{whitespace}{suffix}");
        prop_assume!(!branch.trim().is_empty());
        let request = RunRequest::new(repository, branch)
            .unwrap_or_else(|error| panic!("non-empty request should be constructed: {error}"));

        let error = run_agent(&AppConfig::default(), &request)
            .expect_err("branch whitespace should be rejected");

        let is_branch_error = matches!(
            error,
            PodbotError::Config(ConfigError::InvalidValue { field, .. })
                if field == "run.branch"
        );
        prop_assert!(is_branch_error);
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
#[case::missing_owner_separator("owner-only", "main", "run.repository")]
#[case::empty_owner("/name", "main", "run.repository")]
#[case::empty_name("owner/", "main", "run.repository")]
#[case::owner_contains_whitespace("own er/name", "main", "run.repository")]
#[case::name_contains_whitespace("owner/na me", "main", "run.repository")]
#[case::branch_contains_whitespace("owner/name", "feature branch", "run.branch")]
fn run_agent_rejects_invalid_request_values(
    #[case] repository: &str,
    #[case] branch: &str,
    #[case] expected_field: &str,
) {
    let config = AppConfig::default();
    let request = RunRequest::new(repository, branch)
        .expect("request constructor should accept non-empty values");

    let error = run_agent(&config, &request).expect_err("run_agent should validate request values");

    assert!(matches!(
        error,
        PodbotError::Config(ConfigError::InvalidValue { field, .. })
            if field == expected_field
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

    let command_outcome = run_agent(&config, &request)
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;

    require_outcome(command_outcome, CommandOutcome::Success)?;
    require_equal(request.repository(), repository, "repository")?;
    require_equal(request.branch(), branch, "branch")?;
    Ok(())
}

#[cfg(feature = "experimental")]
fn require_equal(
    actual: &str,
    expected: &str,
    field: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("expected {field} {expected:?}, got {actual:?}").into())
    }
}

#[cfg(feature = "experimental")]
fn require_outcome(
    actual: CommandOutcome,
    expected: CommandOutcome,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if actual == expected {
        Ok(())
    } else {
        Err(Box::new(std::io::Error::other(format!(
            "expected outcome {expected:?}, got {actual:?}"
        ))))
    }
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
fn credential_validation_uses_injected_test_validator() {
    let calls = AtomicUsize::new(0);
    let private_key_path = Utf8PathBuf::from("/tmp/test-key.pem");

    super::validate_agent_github_credentials_with(1, &private_key_path, |_, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    })
    .expect("credential validation should succeed");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[rstest]
#[cfg(feature = "experimental")]
fn credential_validation_supports_concurrent_calls_inside_current_runtime() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = tokio::runtime::Runtime::new().expect("runtime should be created");

    runtime.block_on(async {
        let tasks = (0..4)
            .map(|index| {
                let task_calls = Arc::clone(&calls);
                tokio::spawn(async move {
                    let private_key_path = Utf8PathBuf::from(format!("/tmp/test-key-{index}.pem"));
                    super::validate_agent_github_credentials_with(
                        1,
                        &private_key_path,
                        move |_, _| {
                            task_calls.fetch_add(1, Ordering::SeqCst);
                            Box::pin(async { Ok(()) })
                        },
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

    assert_eq!(calls.load(Ordering::SeqCst), 4);
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
