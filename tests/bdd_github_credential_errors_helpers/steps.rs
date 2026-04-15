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

/// Build a mock error message for a given HTTP status code.
///
/// Mirrors the production `classify_by_status` message format. The
/// `full_error` parameter represents the complete octocrab `Display`
/// output that would be passed to the classifier in production.
fn error_for(status: u16, full_error: &str) -> String {
    match status {
        401 => format!(
            concat!(
                "credentials rejected (HTTP 401). ",
                "Hint: The private key may not match the App, or the App may have been ",
                "suspended. Verify the App ID and regenerate the private key from the ",
                "GitHub App settings page. If the system clock is significantly skewed, ",
                "JWT validation will also fail. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        403 => format!(
            concat!(
                "insufficient permissions (HTTP 403). ",
                "Hint: The App may lack the required permissions. Check the App's ",
                "permission settings in GitHub. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        404 => format!(
            concat!(
                "App not found (HTTP 404). ",
                "Hint: Verify that github.app_id is correct. The App may have been ",
                "deleted. Raw error: {raw}",
            ),
            raw = full_error,
        ),
        500..=599 => format!(
            concat!(
                "GitHub API unavailable (HTTP {code}). ",
                "Hint: Check https://www.githubstatus.com for outage information. ",
                "Retry after the service recovers. Raw error: {raw}",
            ),
            code = status,
            raw = full_error,
        ),
        _ => format!(
            concat!(
                "unexpected response (HTTP {code}). ",
                "Hint: Check https://www.githubstatus.com for outage information. ",
                "Raw error: {raw}",
            ),
            code = status,
            raw = full_error,
        ),
    }
}

/// Build a mock error message for a rate-limit 403 response.
///
/// Separate from `error_for` because rate-limit 403 is a sub-case of
/// HTTP 403 distinguished by the message body, not the status code alone.
fn error_for_rate_limit(full_error: &str) -> String {
    format!(
        concat!(
            "rate limit exceeded (HTTP 403). ",
            "Hint: The GitHub API rate limit has been exceeded. ",
            "Wait a few minutes and retry. Check https://www.githubstatus.com ",
            "if the problem persists. Raw error: {raw}",
        ),
        raw = full_error,
    )
}

/// Create and configure a mock client for the given HTTP response.
fn configure_mock_client(mock_response: MockHttpResponse) -> MockGitHubAppClient {
    let mut mock_client = MockGitHubAppClient::new();
    let message = match mock_response {
        MockHttpResponse::Unauthorized401 => error_for(401, "Bad credentials"),
        MockHttpResponse::Forbidden403 => error_for(403, "Resource not accessible"),
        MockHttpResponse::RateLimited403 => {
            error_for_rate_limit("API rate limit exceeded for installation ID 12345")
        }
        MockHttpResponse::NotFound404 => error_for(404, "Not Found"),
        MockHttpResponse::ServerError503 => error_for(503, "Service unavailable"),
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
    mock_response: MockHttpResponse,
) -> impl FnOnce(u64, EncodingKey) -> Result<MockGitHubAppClient, GitHubError> {
    move |_app_id: u64, _key: EncodingKey| Ok(configure_mock_client(mock_response))
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

#[given("a mock GitHub API that returns HTTP 403 for rate limiting")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn mock_api_rate_limits_403(
    github_credential_errors_state: &GitHubCredentialErrorsState,
) -> StepResult<()> {
    github_credential_errors_state
        .mock_response
        .set(MockHttpResponse::RateLimited403);
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
    let factory = build_factory(inputs.mock_response);
    let outcome = run_validation(inputs.app_id, &inputs.key_path, factory)?;
    github_credential_errors_state.outcome.set(outcome);
    Ok(())
}
