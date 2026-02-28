//! Then step definitions for GitHub App client construction BDD tests.

use rstest_bdd_macros::then;

use super::state::{ClientBuildOutcome, GitHubAppClientState, StepResult};

#[then("the App client is created successfully")]
fn client_created_successfully(github_app_client_state: &GitHubAppClientState) -> StepResult<()> {
    let outcome = github_app_client_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome should be set"))?;
    match outcome {
        ClientBuildOutcome::Success => Ok(()),
        ClientBuildOutcome::Failed { message } => {
            Err(format!("expected successful client build, got: {message}"))
        }
    }
}
