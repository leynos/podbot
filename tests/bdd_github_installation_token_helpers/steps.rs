//! Given and When steps for GitHub installation-token BDD tests.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use podbot::error::GitHubError;
use podbot::github::{
    BoxFuture, GitHubInstallationTokenClient, InstallationAccessToken,
    acquire_installation_token_with_client,
};
use rstest_bdd_macros::{given, when};

use super::state::{
    FIXTURE_TOKEN, GitHubInstallationTokenState, MockTokenResponse, StepResult, TokenOutcome,
};

mockall::mock! {
    pub GitHubInstallationTokenClient {}

    impl GitHubInstallationTokenClient for GitHubInstallationTokenClient {
        fn acquire_installation_token(
            &self,
            installation_id: u64,
            expiry_buffer: Duration,
        ) -> BoxFuture<'_, Result<InstallationAccessToken, GitHubError>>;
    }
}

struct TokenInputs {
    installation_id: u64,
    expiry_buffer: Duration,
    acquired_at: SystemTime,
    mock_response: MockTokenResponse,
    runtime: Arc<tokio::runtime::Runtime>,
}

fn read_inputs(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<TokenInputs> {
    let installation_id = github_installation_token_state
        .installation_id
        .get()
        .ok_or_else(|| String::from("installation_id should be set"))?;
    let expiry_buffer = github_installation_token_state
        .expiry_buffer
        .get()
        .ok_or_else(|| String::from("expiry_buffer should be set"))?;
    let acquired_at = github_installation_token_state
        .acquired_at
        .get()
        .ok_or_else(|| String::from("acquired_at should be set"))?;
    let mock_response = github_installation_token_state
        .mock_response
        .get()
        .ok_or_else(|| String::from("mock_response should be set"))?;
    let runtime = github_installation_token_state
        .runtime
        .get()
        .ok_or_else(|| String::from("runtime should be set"))??;
    Ok(TokenInputs {
        installation_id,
        expiry_buffer,
        acquired_at,
        mock_response,
        runtime,
    })
}

fn configure_mock_client(inputs: &TokenInputs) -> StepResult<MockGitHubInstallationTokenClient> {
    let mut mock_client = MockGitHubInstallationTokenClient::new();
    match inputs.mock_response {
        MockTokenResponse::Success => {
            let expected_installation_id = inputs.installation_id;
            let expected_expiry_buffer = inputs.expiry_buffer;
            let token = InstallationAccessToken::new(
                String::from(FIXTURE_TOKEN),
                inputs.acquired_at,
                inputs.expiry_buffer,
            )
            .map_err(|e| format!("should create fixture token: {e}"))?;
            mock_client
                .expect_acquire_installation_token()
                .withf(move |installation_id, buffer| {
                    *installation_id == expected_installation_id
                        && *buffer == expected_expiry_buffer
                })
                .times(1)
                .return_once(move |_, _| Box::pin(async move { Ok(token) }));
        }
        MockTokenResponse::RejectedInstallation => {
            let expected_installation_id = inputs.installation_id;
            let expected_expiry_buffer = inputs.expiry_buffer;
            mock_client
                .expect_acquire_installation_token()
                .withf(move |installation_id, buffer| {
                    *installation_id == expected_installation_id
                        && *buffer == expected_expiry_buffer
                })
                .times(1)
                .returning(|_, _| {
                    Box::pin(async {
                        Err(GitHubError::TokenAcquisitionFailed {
                            message: String::from("GitHub rejected installation token acquisition"),
                        })
                    })
                });
        }
    }
    Ok(mock_client)
}

fn run_acquisition(inputs: &TokenInputs) -> StepResult<TokenOutcome> {
    let mock_client = configure_mock_client(inputs)?;
    let result = inputs.runtime.block_on(async {
        acquire_installation_token_with_client(
            &mock_client,
            inputs.installation_id,
            inputs.expiry_buffer,
        )
        .await
    });
    Ok(match result {
        Ok(token) => TokenOutcome::Success { token },
        Err(error) => TokenOutcome::Failed {
            message: error.to_string(),
        },
    })
}

fn set_mock_response(
    github_installation_token_state: &GitHubInstallationTokenState,
    response: MockTokenResponse,
) {
    github_installation_token_state.mock_response.set(response);
}

#[given("a mock GitHub installation token API that returns a scoped token")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn mock_api_returns_token(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    set_mock_response(github_installation_token_state, MockTokenResponse::Success);
    Ok(())
}

#[given("a mock GitHub installation token API that rejects the installation")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn mock_api_rejects_installation(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    set_mock_response(
        github_installation_token_state,
        MockTokenResponse::RejectedInstallation,
    );
    Ok(())
}

#[given("the GitHub App installation ID is {installation_id}")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn set_installation_id(
    github_installation_token_state: &GitHubInstallationTokenState,
    installation_id: u64,
) -> StepResult<()> {
    github_installation_token_state
        .installation_id
        .set(installation_id);
    github_installation_token_state
        .acquired_at
        .set(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000));
    Ok(())
}

#[given("the token expiry buffer is {seconds} seconds")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn set_expiry_buffer(
    github_installation_token_state: &GitHubInstallationTokenState,
    seconds: u64,
) -> StepResult<()> {
    github_installation_token_state
        .expiry_buffer
        .set(Duration::from_secs(seconds));
    Ok(())
}

#[when("an installation token is acquired")]
fn acquire_token(github_installation_token_state: &GitHubInstallationTokenState) -> StepResult<()> {
    let inputs = read_inputs(github_installation_token_state)?;
    let outcome = run_acquisition(&inputs)?;
    github_installation_token_state.outcome.set(outcome);
    Ok(())
}
