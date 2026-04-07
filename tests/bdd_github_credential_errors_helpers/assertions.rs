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
    let message = get_failure_message(github_credential_errors_state)?;
    if message.contains("credentials rejected") {
        Ok(())
    } else {
        Err(format!("expected 'credentials rejected' in: {message}"))
    }
}

#[then("the error includes a remediation hint")]
fn error_includes_remediation_hint(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    let message = get_failure_message(github_credential_errors_state)?;
    if message.contains("Hint:") && message.contains("regenerate") {
        Ok(())
    } else {
        Err(format!("expected regeneration hint in: {message}"))
    }
}

#[then("the error mentions not found")]
fn error_mentions_not_found(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    let message = get_failure_message(github_credential_errors_state)?;
    if message.contains("not found") {
        Ok(())
    } else {
        Err(format!("expected 'not found' in: {message}"))
    }
}

#[then("the error includes an app ID verification hint")]
fn error_includes_app_id_hint(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    let message = get_failure_message(github_credential_errors_state)?;
    if message.contains("github.app_id") {
        Ok(())
    } else {
        Err(format!("expected 'github.app_id' hint in: {message}"))
    }
}

#[then("the error mentions unavailable")]
fn error_mentions_unavailable(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    let message = get_failure_message(github_credential_errors_state)?;
    if message.contains("unavailable") {
        Ok(())
    } else {
        Err(format!("expected 'unavailable' in: {message}"))
    }
}

#[then("the error includes a status page hint")]
fn error_includes_status_page_hint(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    let message = get_failure_message(github_credential_errors_state)?;
    if message.contains("githubstatus.com") {
        Ok(())
    } else {
        Err(format!("expected 'githubstatus.com' hint in: {message}"))
    }
}

#[then("the error mentions permissions")]
fn error_mentions_permissions(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    let message = get_failure_message(github_credential_errors_state)?;
    if message.contains("permissions") {
        Ok(())
    } else {
        Err(format!("expected 'permissions' in: {message}"))
    }
}

#[then("the error includes a settings hint")]
fn error_includes_settings_hint(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    let message = get_failure_message(github_credential_errors_state)?;
    if message.contains("permission settings") {
        Ok(())
    } else {
        Err(format!("expected 'permission settings' hint in: {message}"))
    }
}
