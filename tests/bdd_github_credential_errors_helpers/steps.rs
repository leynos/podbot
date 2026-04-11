//! Given and When step definitions for GitHub credential error
//! classification BDD tests.

use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;
use rstest_bdd_macros::{given, when};

use jsonwebtoken::EncodingKey;
use podbot::error::GitHubError;
use podbot::github::{BoxFuture, GitHubAppClient, validate_with_factory};

use super::state::{GitHubCredentialErrorsState, MockHttpResponse, StepResult, ValidationOutcome};

// Define a mock client for testing since the automock is only available
// in the main crate's test configuration.
mockall::mock! {
    pub GitHubAppClient {}

    impl GitHubAppClient for GitHubAppClient {
        fn validate_credentials(&self) -> BoxFuture<'_, Result<(), GitHubError>>;
    }
}

/// Input values extracted from scenario state for validation.
struct ValidationInputs {
    key_path: Utf8PathBuf,
    app_id: u64,
    mock_response: MockHttpResponse,
}

/// Extract validation inputs from scenario state.
fn read_inputs(state: &GitHubCredentialErrorsState) -> StepResult<ValidationInputs> {
    let key_path = state
        .key_path
        .get()
        .ok_or_else(|| String::from("key_path should be set"))?;
    let app_id = state
        .app_id
        .get()
        .ok_or_else(|| String::from("app_id should be set"))?;
    let mock_response = state
        .mock_response
        .get()
        .ok_or_else(|| String::from("mock_response should be set"))?;
    Ok(ValidationInputs {
        key_path,
        app_id,
        mock_response,
    })
}

/// Build a mock error message for 401 responses.
///
/// Returns a minimal error message containing the key substrings that
/// the BDD assertions check for. This avoids duplicating the complete
/// classification wording, which could silently diverge if the classifier
/// changes.
fn error_401() -> String {
    String::from("credentials rejected. Hint: regenerate the private key. clock")
}

/// Build a mock error message for 403 responses.
///
/// Returns a minimal error message containing the key substrings that
/// the BDD assertions check for. This avoids duplicating the complete
/// classification wording, which could silently diverge if the classifier
/// changes.
fn error_403() -> String {
    String::from("permissions. Hint: permission settings")
}

/// Build a mock error message for 404 responses.
///
/// Returns a minimal error message containing the key substrings that
/// the BDD assertions check for. This avoids duplicating the complete
/// classification wording, which could silently diverge if the classifier
/// changes.
fn error_404() -> String {
    String::from("not found. Hint: github.app_id")
}

/// Build a mock error message for 503 responses.
///
/// Returns a minimal error message containing the key substrings that
/// the BDD assertions check for. This avoids duplicating the complete
/// classification wording, which could silently diverge if the classifier
/// changes.
fn error_503() -> String {
    String::from("unavailable. Hint: githubstatus.com")
}

/// Create and configure a mock client for the given HTTP response.
fn configure_mock_client(mock_response: MockHttpResponse) -> MockGitHubAppClient {
    let mut mock_client = MockGitHubAppClient::new();
    let message = match mock_response {
        MockHttpResponse::Unauthorized401 => error_401(),
        MockHttpResponse::Forbidden403 => error_403(),
        MockHttpResponse::NotFound404 => error_404(),
        MockHttpResponse::ServerError503 => error_503(),
    };
    mock_client
        .expect_validate_credentials()
        .times(1)
        .returning(move || {
            let msg = message.clone();
            Box::pin(async move { Err(GitHubError::AuthenticationFailed { message: msg }) })
        });
    mock_client
}

/// Build a factory closure that creates a mock client.
fn build_factory(
    expected_app_id: u64,
    mock_response: MockHttpResponse,
) -> impl FnOnce(u64, EncodingKey) -> Result<MockGitHubAppClient, GitHubError> {
    move |received_app_id: u64, _key: EncodingKey| {
        if received_app_id != expected_app_id {
            return Err(GitHubError::AuthenticationFailed {
                message: format!(
                    "app_id mismatch: expected {expected_app_id}, \
                     received {received_app_id}"
                ),
            });
        }
        Ok(configure_mock_client(mock_response))
    }
}

/// Run validation and convert the result to a `ValidationOutcome`.
fn run_validation(
    app_id: u64,
    key_path: &Utf8Path,
    factory: impl FnOnce(u64, EncodingKey) -> Result<MockGitHubAppClient, GitHubError>,
) -> StepResult<ValidationOutcome> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;
    let result = rt.block_on(async { validate_with_factory(app_id, key_path, factory).await });
    Ok(match result {
        Ok(()) => ValidationOutcome::Success,
        Err(error) => ValidationOutcome::Failed {
            message: error.to_string(),
        },
    })
}

/// Open a temporary directory as a `cap_std` capability handle.
fn open_temp_dir() -> StepResult<(tempfile::TempDir, Dir, Utf8PathBuf)> {
    let tmp = tempfile::tempdir().map_err(|e| format!("should create temp dir: {e}"))?;
    let tmp_path = Utf8Path::from_path(tmp.path())
        .ok_or_else(|| String::from("temp dir path should be UTF-8"))?
        .to_owned();
    let dir = Dir::open_ambient_dir(&tmp_path, ambient_authority())
        .map_err(|e| format!("should open temp dir: {e}"))?;
    Ok((tmp, dir, tmp_path))
}

#[given("a mock GitHub API that rejects credentials with HTTP 401")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn mock_api_rejects_401(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    github_credential_errors_state
        .mock_response
        .set(MockHttpResponse::Unauthorized401);
    Ok(())
}

#[given("a mock GitHub API that returns HTTP 403")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn mock_api_returns_403(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    github_credential_errors_state
        .mock_response
        .set(MockHttpResponse::Forbidden403);
    Ok(())
}

#[given("a mock GitHub API that returns HTTP 404")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn mock_api_returns_404(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    github_credential_errors_state
        .mock_response
        .set(MockHttpResponse::NotFound404);
    Ok(())
}

#[given("a mock GitHub API that returns HTTP 503")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn mock_api_returns_503(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    github_credential_errors_state
        .mock_response
        .set(MockHttpResponse::ServerError503);
    Ok(())
}

#[given("a valid RSA private key file exists at the configured path")]
fn valid_rsa_key_file(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    let pem = include_str!("../fixtures/test_rsa_private_key.pem");
    let (tmp, dir, tmp_path) = open_temp_dir()?;
    dir.write("key.pem", pem)
        .map_err(|e| format!("should write key file: {e}"))?;
    github_credential_errors_state.temp_dir.set(Arc::new(tmp));
    github_credential_errors_state
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
    github_credential_errors_state: &GitHubCredentialErrorsState,
    app_id: u64,
) -> StepResult<()> {
    github_credential_errors_state.app_id.set(app_id);
    Ok(())
}

#[when("credentials are validated")]
fn validate_credentials_step(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    let inputs = read_inputs(github_credential_errors_state)?;
    let factory = build_factory(inputs.app_id, inputs.mock_response);
    let outcome = run_validation(inputs.app_id, &inputs.key_path, factory)?;
    github_credential_errors_state.outcome.set(outcome);
    Ok(())
}
