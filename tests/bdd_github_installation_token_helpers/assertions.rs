//! Then step definitions for installation-token scenarios.

use rstest_bdd_macros::then;

use super::state::{GitHubInstallationTokenState, InstallationTokenOutcome, StepResult};

fn outcome(state: &GitHubInstallationTokenState) -> StepResult<InstallationTokenOutcome> {
    state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))
}

fn expect_failure_message(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<String> {
    match outcome(github_installation_token_state)? {
        InstallationTokenOutcome::Success { .. } => {
            Err(String::from("expected token acquisition to fail"))
        }
        InstallationTokenOutcome::Failed { message } => Ok(message),
    }
}

fn assert_failure_message_contains(
    github_installation_token_state: &GitHubInstallationTokenState,
    needle: &str,
    description: &str,
) -> StepResult<()> {
    let message = expect_failure_message(github_installation_token_state)?;
    if message.contains(needle) {
        Ok(())
    } else {
        Err(format!(
            "expected error to mention {description}, got: {message}"
        ))
    }
}

fn expect_success_token_fields(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<(String, String)> {
    match outcome(github_installation_token_state)? {
        InstallationTokenOutcome::Success { token, expires_at } => Ok((token, expires_at)),
        InstallationTokenOutcome::Failed { message } => {
            Err(format!("expected success, got failure: {message}"))
        }
    }
}

#[then("installation token acquisition succeeds")]
fn installation_token_acquisition_succeeds(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    match outcome(github_installation_token_state)? {
        InstallationTokenOutcome::Success { .. } => Ok(()),
        InstallationTokenOutcome::Failed { message } => Err(format!(
            "expected token acquisition to succeed, got: {message}"
        )),
    }
}

#[then("the returned token is exposed for Git operations")]
fn token_is_exposed_for_git_operations(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    let (token, _) = expect_success_token_fields(github_installation_token_state)?;
    if token == "ghs_valid_bdd" {
        Ok(())
    } else {
        Err(format!("unexpected token value: {token}"))
    }
}

#[then("the returned expiry metadata is preserved")]
fn expiry_metadata_is_preserved(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    let (_, expires_at) = expect_success_token_fields(github_installation_token_state)?;
    if expires_at == "2099-01-01T00:10:00+00:00" {
        Ok(())
    } else {
        Err(format!("unexpected expiry timestamp: {expires_at}"))
    }
}

#[then("installation token acquisition fails with token expired")]
fn installation_token_acquisition_fails_with_token_expired(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    let message = expect_failure_message(github_installation_token_state)?;
    if message == "installation token expired" {
        Ok(())
    } else {
        Err(format!(
            "expected 'installation token expired', got: {message}"
        ))
    }
}

#[then("the error mentions installation not found")]
fn error_mentions_installation_not_found(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    assert_failure_message_contains(
        github_installation_token_state,
        "installation not found",
        "'installation not found'",
    )
}

#[then("the error mentions missing expires_at metadata")]
fn error_mentions_missing_expiry_metadata(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    assert_failure_message_contains(
        github_installation_token_state,
        "did not include expires_at",
        "missing expires_at metadata",
    )
}
