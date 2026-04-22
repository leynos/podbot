//! Then step definitions for installation-token scenarios.

use rstest_bdd_macros::then;

use super::state::{GitHubInstallationTokenState, InstallationTokenOutcome, StepResult};

fn outcome(state: &GitHubInstallationTokenState) -> StepResult<InstallationTokenOutcome> {
    state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))
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
    match outcome(github_installation_token_state)? {
        InstallationTokenOutcome::Success { token, .. } => {
            if token == "ghs_valid_bdd" {
                Ok(())
            } else {
                Err(format!("unexpected token value: {token}"))
            }
        }
        InstallationTokenOutcome::Failed { message } => {
            Err(format!("expected success, got failure: {message}"))
        }
    }
}

#[then("the returned expiry metadata is preserved")]
fn expiry_metadata_is_preserved(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    match outcome(github_installation_token_state)? {
        InstallationTokenOutcome::Success { expires_at, .. } => {
            if expires_at == "2099-01-01T00:10:00+00:00" {
                Ok(())
            } else {
                Err(format!("unexpected expiry timestamp: {expires_at}"))
            }
        }
        InstallationTokenOutcome::Failed { message } => {
            Err(format!("expected success, got failure: {message}"))
        }
    }
}

#[then("installation token acquisition fails with token expired")]
fn installation_token_acquisition_fails_with_token_expired(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    match outcome(github_installation_token_state)? {
        InstallationTokenOutcome::Success { .. } => {
            Err(String::from("expected token acquisition to fail"))
        }
        InstallationTokenOutcome::Failed { message } => {
            if message == "installation token expired" {
                Ok(())
            } else {
                Err(format!(
                    "expected 'installation token expired', got: {message}"
                ))
            }
        }
    }
}

#[then("the error mentions installation not found")]
fn error_mentions_installation_not_found(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    match outcome(github_installation_token_state)? {
        InstallationTokenOutcome::Success { .. } => {
            Err(String::from("expected token acquisition to fail"))
        }
        InstallationTokenOutcome::Failed { message } => {
            if message.contains("installation not found") {
                Ok(())
            } else {
                Err(format!(
                    "expected error to mention installation not found, got: {message}"
                ))
            }
        }
    }
}

#[then("the error mentions missing expires_at metadata")]
fn error_mentions_missing_expiry_metadata(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    match outcome(github_installation_token_state)? {
        InstallationTokenOutcome::Success { .. } => {
            Err(String::from("expected token acquisition to fail"))
        }
        InstallationTokenOutcome::Failed { message } => {
            if message.contains("did not include expires_at") {
                Ok(())
            } else {
                Err(format!(
                    "expected error to mention missing expires_at metadata, got: {message}"
                ))
            }
        }
    }
}
