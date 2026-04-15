//! Then step definitions for GitHub credential error classification
//! BDD tests.

use rstest_bdd_macros::then;

use super::state::{GitHubCredentialErrorsState, StepResult, ValidationOutcome};

/// Extract the validation outcome from state, returning an error if
/// not set.
fn get_outcome(state: &GitHubCredentialErrorsState) -> StepResult<ValidationOutcome> {
    state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))
}

/// Extract the failure message, or return an error if validation
/// succeeded.
fn get_failure_message(state: &GitHubCredentialErrorsState) -> StepResult<String> {
    match get_outcome(state)? {
        ValidationOutcome::Success => Err(String::from("expected validation to fail")),
        ValidationOutcome::Failed { message } => Ok(message),
    }
}

/// Assert that the failure message contains all expected substrings.
fn assert_message_contains_all(
    state: &GitHubCredentialErrorsState,
    needles: &[&str],
    description: &str,
) -> StepResult<()> {
    let message = get_failure_message(state)?;
    if needles.iter().all(|n| message.contains(n)) {
        Ok(())
    } else {
        Err(format!("expected {description} in: {message}"))
    }
}

#[then("validation fails")]
fn validation_fails(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    let _ = get_failure_message(github_credential_errors_state)?;
    Ok(())
}

#[then("the error mentions credentials rejected")]
fn error_mentions_credentials_rejected(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    assert_message_contains_all(
        github_credential_errors_state,
        &["credentials rejected"],
        "'credentials rejected'",
    )
}

#[then("the error includes a remediation hint")]
fn error_includes_remediation_hint(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    assert_message_contains_all(
        github_credential_errors_state,
        &["Hint:", "regenerate"],
        "regeneration hint",
    )
}

#[then("the error mentions not found")]
fn error_mentions_not_found(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    assert_message_contains_all(
        github_credential_errors_state,
        &["not found"],
        "'not found'",
    )
}

#[then("the error includes an app ID verification hint")]
fn error_includes_app_id_hint(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    assert_message_contains_all(
        github_credential_errors_state,
        &["github.app_id"],
        "'github.app_id' hint",
    )
}

#[then("the error mentions unavailable")]
fn error_mentions_unavailable(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    assert_message_contains_all(
        github_credential_errors_state,
        &["unavailable"],
        "'unavailable'",
    )
}

#[then("the error includes a status page hint")]
fn error_includes_status_page_hint(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    assert_message_contains_all(
        github_credential_errors_state,
        &["githubstatus.com"],
        "'githubstatus.com' hint",
    )
}

#[then("the error mentions rate limit")]
fn error_mentions_rate_limit(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    assert_message_contains_all(
        github_credential_errors_state,
        &["rate limit exceeded"],
        "'rate limit exceeded'",
    )
}

#[then("the error includes a retry hint")]
fn error_includes_retry_hint(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    assert_message_contains_all(
        github_credential_errors_state,
        &["Wait", "retry"],
        "retry hint",
    )
}

#[then("the error mentions permissions")]
fn error_mentions_permissions(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    assert_message_contains_all(
        github_credential_errors_state,
        &["permissions"],
        "'permissions'",
    )
}

#[then("the error includes a settings hint")]
fn error_includes_settings_hint(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    assert_message_contains_all(
        github_credential_errors_state,
        &["permission settings"],
        "'permission settings' hint",
    )
}
