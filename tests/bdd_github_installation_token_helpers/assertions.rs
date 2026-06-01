//! Then steps for GitHub installation-token BDD tests.

use rstest_bdd_macros::then;

use super::state::{FIXTURE_TOKEN, GitHubInstallationTokenState, StepResult, TokenOutcome};
use podbot::github::InstallationAccessToken;

fn get_outcome(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<TokenOutcome> {
    github_installation_token_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))
}

fn require_success(outcome: TokenOutcome) -> StepResult<InstallationAccessToken> {
    match outcome {
        TokenOutcome::Success { token } => Ok(token),
        TokenOutcome::Failed { message } => Err(format!(
            "expected token acquisition to succeed, got: {message}"
        )),
    }
}

fn require_failure(outcome: TokenOutcome) -> StepResult<String> {
    match outcome {
        TokenOutcome::Success { token } => Err(format!(
            "expected token acquisition to fail, got: {token:?}"
        )),
        TokenOutcome::Failed { message } => Ok(message),
    }
}

#[then("token acquisition succeeds")]
fn token_acquisition_succeeds(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    require_success(get_outcome(github_installation_token_state)?).map(|_| ())
}

#[then("token acquisition fails")]
fn token_acquisition_fails(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    require_failure(get_outcome(github_installation_token_state)?).map(|_| ())
}

#[then("the token string is available for Git operations")]
fn token_string_available(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    let token = require_success(get_outcome(github_installation_token_state)?)?;
    if token.token() == FIXTURE_TOKEN {
        Ok(())
    } else {
        Err(format!("expected fixture token, got: {token:?}"))
    }
}

#[then("expiry metadata includes the configured buffer")]
fn expiry_metadata_includes_buffer(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    let expiry_buffer = github_installation_token_state
        .expiry_buffer
        .get()
        .ok_or_else(|| String::from("expiry_buffer should be set"))?;
    let token = require_success(get_outcome(github_installation_token_state)?)?;
    if token.refresh_after() == token.expires_at() - expiry_buffer {
        Ok(())
    } else {
        Err(format!(
            "expected refresh_after to equal expires_at minus {expiry_buffer:?}"
        ))
    }
}

#[then("the error mentions installation token acquisition")]
fn error_mentions_token_acquisition(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    let message = require_failure(get_outcome(github_installation_token_state)?)?;
    if message.contains("installation token acquisition") {
        Ok(())
    } else {
        Err(format!(
            "expected error to mention installation token acquisition, got: {message}"
        ))
    }
}

#[then("observable token metadata does not expose the token string")]
fn observable_metadata_redacts_token(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    let observed = match get_outcome(github_installation_token_state)? {
        TokenOutcome::Success { token } => format!("{token:?}"),
        TokenOutcome::Failed { message } => message,
    };

    if observed.contains(FIXTURE_TOKEN) {
        Err(format!("observable metadata exposed token: {observed}"))
    } else {
        Ok(())
    }
}
