//! Then steps for GitHub installation-token BDD tests.

use rstest_bdd_macros::then;

use super::state::{FIXTURE_TOKEN, GitHubInstallationTokenState, StepResult, TokenOutcome};

fn get_outcome(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<TokenOutcome> {
    github_installation_token_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))
}

#[then("token acquisition succeeds")]
fn token_acquisition_succeeds(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    match get_outcome(github_installation_token_state)? {
        TokenOutcome::Success { .. } => Ok(()),
        TokenOutcome::Failed { message } => Err(format!(
            "expected token acquisition to succeed, got: {message}"
        )),
    }
}

#[then("token acquisition fails")]
fn token_acquisition_fails(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    match get_outcome(github_installation_token_state)? {
        TokenOutcome::Success { token } => Err(format!(
            "expected token acquisition to fail, got: {token:?}"
        )),
        TokenOutcome::Failed { .. } => Ok(()),
    }
}

#[then("the token string is available for Git operations")]
fn token_string_available(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    match get_outcome(github_installation_token_state)? {
        TokenOutcome::Success { token } if token.token() == FIXTURE_TOKEN => Ok(()),
        TokenOutcome::Success { token } => {
            Err(format!("expected fixture token, got: {}", token.token()))
        }
        TokenOutcome::Failed { message } => Err(format!(
            "expected token acquisition to succeed, got: {message}"
        )),
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
    match get_outcome(github_installation_token_state)? {
        TokenOutcome::Success { token } => {
            if token.refresh_after() == token.expires_at() - expiry_buffer {
                Ok(())
            } else {
                Err(format!(
                    "expected refresh_after to equal expires_at minus {expiry_buffer:?}"
                ))
            }
        }
        TokenOutcome::Failed { message } => Err(format!(
            "expected token acquisition to succeed, got: {message}"
        )),
    }
}

#[then("the error mentions installation token acquisition")]
fn error_mentions_token_acquisition(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    match get_outcome(github_installation_token_state)? {
        TokenOutcome::Success { token } => Err(format!(
            "expected token acquisition to fail, got: {token:?}"
        )),
        TokenOutcome::Failed { message } if message.contains("installation token acquisition") => {
            Ok(())
        }
        TokenOutcome::Failed { message } => Err(format!(
            "expected error to mention installation token acquisition, got: {message}"
        )),
    }
}

#[then("observable token metadata does not expose the token string")]
fn observable_metadata_redacts_token(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    let observed = match get_outcome(github_installation_token_state)? {
        TokenOutcome::Success { token } => format!(
            "{:?} {:?}",
            token,
            token.log_fields(
                github_installation_token_state
                    .installation_id
                    .get()
                    .ok_or_else(|| String::from("installation_id should be set"))?,
                github_installation_token_state
                    .expiry_buffer
                    .get()
                    .ok_or_else(|| String::from("expiry_buffer should be set"))?,
            )
        ),
        TokenOutcome::Failed { message } => message,
    };

    if observed.contains(FIXTURE_TOKEN) {
        Err(format!("observable metadata exposed token: {observed}"))
    } else {
        Ok(())
    }
}
