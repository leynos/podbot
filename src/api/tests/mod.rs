//! Unit tests for the orchestration API module.

use proptest::prelude::*;
use rstest::rstest;

use super::{CommandOutcome, RunRequest};
#[cfg(feature = "experimental")]
use super::{validate_github_config_for_run, validate_run_request_for_agent};
#[cfg(feature = "experimental")]
use crate::config::{AppConfig, GitHubConfig};
#[cfg(feature = "experimental")]
use crate::error::{ConfigError, PodbotError};
#[cfg(feature = "experimental")]
use camino::Utf8PathBuf;

mod exec;
#[cfg(feature = "experimental")]
mod repository;

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

        prop_assert!(validate_run_request_for_agent(&request).is_ok());
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

        let error = validate_run_request_for_agent(&request)
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

        let error = validate_run_request_for_agent(&request)
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

    let error = validate_github_config_for_run(&config, &request)
        .expect_err("incomplete GitHub config should be rejected");

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
    let request = RunRequest::new(repository, branch)
        .expect("request constructor should accept non-empty values");

    let error = validate_run_request_for_agent(&request)
        .expect_err("run_agent should validate request values");

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
fn run_agent_accepts_distinct_run_requests(#[case] repository: &str, #[case] branch: &str) {
    let request = RunRequest::new(repository, branch).expect("valid request should be created");

    validate_run_request_for_agent(&request).expect("valid request should be accepted");

    assert_eq!(request.repository(), repository);
    assert_eq!(request.branch(), branch);
}
