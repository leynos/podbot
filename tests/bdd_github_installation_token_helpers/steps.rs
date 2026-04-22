//! Given and When step definitions for installation-token scenarios.

use std::sync::Arc;
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;
use rstest_bdd_macros::{given, when};
use serde_json::json;

use jsonwebtoken::EncodingKey;
use octocrab::models::InstallationToken;
use podbot::error::GitHubError;
use podbot::github::{
    BoxFuture, GitHubInstallationTokenClient, InstallationTokenRequest,
    installation_token_with_factory,
};

use super::state::{
    GitHubInstallationTokenState, InstallationTokenOutcome, MockInstallationTokenResponse,
    StepResult,
};

mockall::mock! {
    pub GitHubInstallationTokenClient {}

    impl GitHubInstallationTokenClient for GitHubInstallationTokenClient {
        fn acquire_installation_token(
            &self,
            installation_id: u64,
        ) -> BoxFuture<'_, Result<InstallationToken, GitHubError>>;
    }
}

struct InstallationTokenInputs {
    key_path: Utf8PathBuf,
    app_id: u64,
    installation_id: u64,
    buffer: Duration,
    mock_response: MockInstallationTokenResponse,
}

fn read_inputs(state: &GitHubInstallationTokenState) -> StepResult<InstallationTokenInputs> {
    let key_path = state
        .key_path
        .get()
        .ok_or_else(|| String::from("key_path should be set"))?;
    let app_id = state
        .app_id
        .get()
        .ok_or_else(|| String::from("app_id should be set"))?;
    let installation_id = state
        .installation_id
        .get()
        .ok_or_else(|| String::from("installation_id should be set"))?;
    let buffer = state
        .buffer
        .get()
        .ok_or_else(|| String::from("buffer should be set"))?;
    let mock_response = state
        .mock_response
        .get()
        .ok_or_else(|| String::from("mock_response should be set"))?;

    Ok(InstallationTokenInputs {
        key_path,
        app_id,
        installation_id,
        buffer,
        mock_response,
    })
}

fn configure_mock_client(
    installation_id: u64,
    mock_response: MockInstallationTokenResponse,
) -> MockGitHubInstallationTokenClient {
    let mut mock_client = MockGitHubInstallationTokenClient::new();
    mock_client
        .expect_acquire_installation_token()
        .times(1)
        .with(mockall::predicate::eq(installation_id))
        .returning(move |_| {
            let result = match mock_response {
                MockInstallationTokenResponse::Success => mock_installation_token(
                    "ghs_valid_bdd",
                    Some("2099-01-01T00:10:00Z"),
                )
                .map_err(|message| GitHubError::TokenAcquisitionFailed { message }),
                MockInstallationTokenResponse::NearExpiry => mock_installation_token(
                    "ghs_near_expiry",
                    Some("2000-01-01T00:01:00Z"),
                )
                .map_err(|message| GitHubError::TokenAcquisitionFailed { message }),
                MockInstallationTokenResponse::ApiRejected => {
                    Err(GitHubError::TokenAcquisitionFailed {
                        message: String::from(
                            "GitHub installation not found (HTTP 404). Hint: Verify github.installation_id.",
                        ),
                    })
                }
                MockInstallationTokenResponse::MissingExpiry => {
                    mock_installation_token("ghs_missing_expiry", None)
                        .map_err(|message| GitHubError::TokenAcquisitionFailed { message })
                }
            };
            Box::pin(async move { result })
        });
    mock_client
}

fn build_factory(
    expected_app_id: u64,
    expected_installation_id: u64,
    mock_response: MockInstallationTokenResponse,
) -> impl FnOnce(u64, EncodingKey) -> Result<MockGitHubInstallationTokenClient, GitHubError> {
    move |received_app_id: u64, _key: EncodingKey| {
        if received_app_id != expected_app_id {
            return Err(GitHubError::AuthenticationFailed {
                message: format!(
                    "app_id mismatch: expected {expected_app_id}, received {received_app_id}"
                ),
            });
        }

        Ok(configure_mock_client(
            expected_installation_id,
            mock_response,
        ))
    }
}

fn run_acquisition(
    inputs: &InstallationTokenInputs,
    factory: impl FnOnce(u64, EncodingKey) -> Result<MockGitHubInstallationTokenClient, GitHubError>,
) -> StepResult<InstallationTokenOutcome> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("failed to create tokio runtime: {error}"))?;

    let result = runtime.block_on(async {
        installation_token_with_factory(
            InstallationTokenRequest::new(
                inputs.app_id,
                inputs.installation_id,
                &inputs.key_path,
                inputs.buffer,
            ),
            factory,
        )
        .await
    });

    Ok(match result {
        Ok(access_token) => {
            let expires_at = access_token.expires_at().to_rfc3339();
            let token = access_token.into_token();
            InstallationTokenOutcome::Success { token, expires_at }
        }
        Err(error) => InstallationTokenOutcome::Failed {
            message: error.to_string(),
        },
    })
}

fn open_temp_dir() -> StepResult<(tempfile::TempDir, Dir, Utf8PathBuf)> {
    let tmp = tempfile::tempdir().map_err(|error| format!("should create temp dir: {error}"))?;
    let tmp_path = Utf8Path::from_path(tmp.path())
        .ok_or_else(|| String::from("temp dir path should be UTF-8"))?
        .to_owned();
    let dir = Dir::open_ambient_dir(&tmp_path, ambient_authority())
        .map_err(|error| format!("should open temp dir: {error}"))?;
    Ok((tmp, dir, tmp_path))
}

fn mock_installation_token(
    token: &str,
    expires_at: Option<&str>,
) -> Result<InstallationToken, String> {
    serde_json::from_value(json!({
        "token": token,
        "expires_at": expires_at,
        "permissions": {},
        "repositories": null,
    }))
    .map_err(|error| format!("test installation token JSON should deserialize: {error}"))
}

fn set_mock_response(
    state: &GitHubInstallationTokenState,
    response: MockInstallationTokenResponse,
) {
    state.mock_response.set(response);
}

#[given("a valid RSA private key file exists at the configured path")]
fn valid_rsa_key_file(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    let pem = include_str!("../fixtures/test_rsa_private_key.pem");
    let (tmp, dir, tmp_path) = open_temp_dir()?;
    dir.write("key.pem", pem)
        .map_err(|error| format!("should write key file: {error}"))?;
    github_installation_token_state.temp_dir.set(Arc::new(tmp));
    github_installation_token_state
        .key_path
        .set(tmp_path.join("key.pem"));
    Ok(())
}

#[given("the GitHub App ID is {app_id}")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn set_app_id(
    github_installation_token_state: &GitHubInstallationTokenState,
    app_id: u64,
) -> StepResult<()> {
    github_installation_token_state.app_id.set(app_id);
    Ok(())
}

#[given("the GitHub installation ID is {installation_id}")]
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
    Ok(())
}

#[given("the expiry buffer is {seconds} seconds")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn set_buffer(
    github_installation_token_state: &GitHubInstallationTokenState,
    seconds: u64,
) -> StepResult<()> {
    github_installation_token_state
        .buffer
        .set(Duration::from_secs(seconds));
    Ok(())
}

#[given("GitHub returns a valid installation token")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn github_returns_valid_installation_token(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    set_mock_response(
        github_installation_token_state,
        MockInstallationTokenResponse::Success,
    );
    Ok(())
}

#[given("GitHub returns an installation token that expires inside the buffer")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn github_returns_near_expiry_token(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    set_mock_response(
        github_installation_token_state,
        MockInstallationTokenResponse::NearExpiry,
    );
    Ok(())
}

#[given("GitHub rejects installation token acquisition")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn github_rejects_installation_token_request(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    set_mock_response(
        github_installation_token_state,
        MockInstallationTokenResponse::ApiRejected,
    );
    Ok(())
}

#[given("GitHub omits the installation token expiry metadata")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn github_omits_expiry_metadata(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    set_mock_response(
        github_installation_token_state,
        MockInstallationTokenResponse::MissingExpiry,
    );
    Ok(())
}

#[when("the installation token is requested")]
fn request_installation_token(
    github_installation_token_state: &GitHubInstallationTokenState,
) -> StepResult<()> {
    let inputs = read_inputs(github_installation_token_state)?;
    let factory = build_factory(inputs.app_id, inputs.installation_id, inputs.mock_response);
    let outcome = run_acquisition(&inputs, factory)?;
    github_installation_token_state.outcome.set(outcome);
    Ok(())
}
