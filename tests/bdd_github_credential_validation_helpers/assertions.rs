//! Then step definitions for GitHub credential validation BDD tests.

use rstest_bdd_macros::then;

use super::state::{GitHubCredentialValidationState, StepResult, ValidationOutcome};

#[then("validation succeeds")]
fn validation_succeeds(
    github_credential_validation_state: &GitHubCredentialValidationState,
) -> StepResult<()> {
    let outcome = github_credential_validation_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))?;
    match outcome {
        ValidationOutcome::Success => Ok(()),
        ValidationOutcome::Failed { message } => {
            Err(format!("expected validation to succeed, got: {message}"))
        }
    }
}

#[then("validation fails")]
fn validation_fails(
    github_credential_validation_state: &GitHubCredentialValidationState,
) -> StepResult<()> {
    let outcome = github_credential_validation_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))?;
    match outcome {
        ValidationOutcome::Success => Err(String::from("expected validation to fail")),
        ValidationOutcome::Failed { .. } => Ok(()),
    }
}

#[then("the error mentions invalid credentials")]
fn error_mentions_invalid_credentials(
    github_credential_validation_state: &GitHubCredentialValidationState,
) -> StepResult<()> {
    let outcome = github_credential_validation_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))?;
    match outcome {
        ValidationOutcome::Success => Err(String::from("expected validation to fail")),
        ValidationOutcome::Failed { message } => {
            if message.contains("invalid credentials") {
                Ok(())
            } else {
                Err(format!(
                    "expected error to mention 'invalid credentials', got: {message}"
                ))
            }
        }
    }
}

#[then("the error mentions failed to validate")]
fn error_mentions_failed_to_validate(
    github_credential_validation_state: &GitHubCredentialValidationState,
) -> StepResult<()> {
    let outcome = github_credential_validation_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))?;
    match outcome {
        ValidationOutcome::Success => Err(String::from("expected validation to fail")),
        ValidationOutcome::Failed { message } => {
            if message.contains("failed to validate") {
                Ok(())
            } else {
                Err(format!(
                    "expected error to mention 'failed to validate', got: {message}"
                ))
            }
        }
    }
}
