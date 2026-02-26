//! Then step definitions for GitHub private key loading BDD tests.

use rstest_bdd_macros::then;

use super::state::{GitHubPrivateKeyState, KeyLoadOutcome, StepResult};

#[then("the private key loads successfully")]
fn key_loads_successfully(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let outcome = github_private_key_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))?;
    match outcome {
        KeyLoadOutcome::Success => Ok(()),
        KeyLoadOutcome::Failed { message } => {
            Err(format!("expected successful key load, got: {message}"))
        }
    }
}

#[then("the private key load fails")]
fn key_load_fails(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let outcome = github_private_key_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))?;
    match outcome {
        KeyLoadOutcome::Failed { .. } => Ok(()),
        KeyLoadOutcome::Success => Err(String::from("expected key load failure, got success")),
    }
}

#[then("the error mentions {expected}")]
fn error_mentions(
    github_private_key_state: &GitHubPrivateKeyState,
    expected: String,
) -> StepResult<()> {
    let outcome = github_private_key_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))?;
    match outcome {
        KeyLoadOutcome::Failed { message } => {
            if message.contains(&expected) {
                Ok(())
            } else {
                Err(format!(
                    "expected error to contain '{expected}', got: {message}"
                ))
            }
        }
        KeyLoadOutcome::Success => Err(String::from("expected failure but got success")),
    }
}
