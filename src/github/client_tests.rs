//! Unit tests for Octocrab App client construction, credential validation,
//! and retry metric status classification.

use std::io;

use eyre::{bail, ensure};

use cap_std::fs_utf8::Dir as Utf8Dir;
use rstest::rstest;
use tempfile::TempDir;

use super::super::retry_metrics::github_status_class;
use super::super::*;
use super::{ec_pem, temp_key_dir, valid_rsa_pem};

#[rstest]
fn build_app_client_with_valid_key_succeeds(
    valid_rsa_pem: String,
    temp_key_dir: io::Result<(TempDir, Utf8Dir)>,
) -> eyre::Result<()> {
    let (_tmp, dir) = temp_key_dir?;
    dir.write("key.pem", &valid_rsa_pem)?;
    let path = Utf8Path::new("/display/key.pem");
    let key = load_private_key_from_dir(&dir, "key.pem", path).expect("should load valid key");
    // Octocrab's build() spawns a Tower buffer task requiring a Tokio runtime.
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();
    let result = build_app_client(12345, key);
    ensure!(result.is_ok(), "expected Ok, got: {result:?}");
    Ok(())
}

#[rstest]
fn build_app_client_with_zero_app_id_succeeds(
    valid_rsa_pem: String,
    temp_key_dir: io::Result<(TempDir, Utf8Dir)>,
) -> eyre::Result<()> {
    let (_tmp, dir) = temp_key_dir?;
    dir.write("key.pem", &valid_rsa_pem)?;
    let path = Utf8Path::new("/display/key.pem");
    let key = load_private_key_from_dir(&dir, "key.pem", path).expect("should load valid key");
    // Builder does not validate app_id; GitHub validates at token time.
    // Octocrab's build() spawns a Tower buffer task requiring a Tokio runtime.
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();
    let result = build_app_client(0, key);
    ensure!(
        result.is_ok(),
        "expected Ok even with zero app_id, got: {result:?}"
    );
    Ok(())
}

#[rstest]
fn build_app_client_without_runtime_returns_error(
    valid_rsa_pem: String,
    temp_key_dir: io::Result<(TempDir, Utf8Dir)>,
) -> eyre::Result<()> {
    let (_tmp, dir) = temp_key_dir?;
    dir.write("key.pem", &valid_rsa_pem)?;
    let path = Utf8Path::new("/display/key.pem");
    let key = load_private_key_from_dir(&dir, "key.pem", path).expect("should load valid key");
    // Call without entering a Tokio runtime — should return Err, not panic.
    let result = build_app_client(42, key);
    ensure!(result.is_err(), "expected Err without runtime, got Ok");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    ensure!(
        message.contains("no Tokio runtime context"),
        "error should mention missing runtime: {message}"
    );
    Ok(())
}

#[rstest]
#[case::client_error(http::StatusCode::TOO_MANY_REQUESTS, "4xx")]
#[case::server_error(http::StatusCode::INTERNAL_SERVER_ERROR, "5xx")]
#[case::redirect(http::StatusCode::TEMPORARY_REDIRECT, "3xx")]
fn github_status_class_groups_status_codes(
    #[case] status_code: http::StatusCode,
    #[case] expected_class: &str,
) {
    assert_eq!(github_status_class(status_code), expected_class);
}

#[rstest]
#[case::builder_context(
    "failed to build GitHub App client: test error",
    "failed to build GitHub App client"
)]
#[case::validation_context(
    "failed to validate GitHub App credentials: test error",
    "failed to validate GitHub App credentials"
)]
fn authentication_failed_error_includes_context(
    #[case] message: &str,
    #[case] expected_context: &str,
) {
    let error = GitHubError::AuthenticationFailed {
        message: String::from(message),
    };
    let display = error.to_string();
    assert!(
        display.contains(expected_context),
        "error should include context: {display}"
    );
    assert!(
        display.contains("test error"),
        "error should include cause: {display}"
    );
}

#[rstest]
#[tokio::test]
async fn validate_app_credentials_with_missing_key_returns_error(
    temp_key_dir: io::Result<(TempDir, Utf8Dir)>,
) -> eyre::Result<()> {
    let (temp_dir, _dir) = temp_key_dir?;
    let key_path = Utf8Path::from_path(temp_dir.path())
        .expect("temp dir path should be UTF-8")
        .join("key.pem");
    let result = validate_app_credentials(12345, &key_path).await;
    match result {
        Err(GitHubError::PrivateKeyLoadFailed { ref path, .. }) => {
            ensure!(
                path.to_string_lossy().contains("key.pem"),
                "error path should reference the missing file"
            );
        }
        other => bail!("expected PrivateKeyLoadFailed, got: {other:?}"),
    }
    Ok(())
}

#[rstest]
#[tokio::test]
async fn validate_app_credentials_with_invalid_pem_returns_error(
    ec_pem: String,
    temp_key_dir: io::Result<(TempDir, Utf8Dir)>,
) -> eyre::Result<()> {
    let (tmp, dir) = temp_key_dir?;
    dir.write("ec.pem", &ec_pem)?;
    let full_path = tmp.path().join("ec.pem");
    let utf8_path = Utf8Path::from_path(&full_path).expect("temp path should be UTF-8");

    let result = validate_app_credentials(12345, utf8_path).await;
    match result {
        Err(GitHubError::PrivateKeyLoadFailed { message, .. }) => {
            ensure!(
                message.contains("ECDSA"),
                "error should mention ECDSA: {message}"
            );
        }
        other => bail!("expected PrivateKeyLoadFailed, got: {other:?}"),
    }
    Ok(())
}

#[rstest]
#[tokio::test]
async fn validate_with_client_propagates_mock_success() {
    let mut mock = MockGitHubAppClient::new();
    mock.expect_validate_credentials()
        .times(1)
        .returning(|| Box::pin(async { Ok(()) }));

    let result = validate_with_client(&mock).await;
    assert!(result.is_ok(), "expected Ok from mock client");
}

#[rstest]
#[tokio::test]
async fn validate_with_client_propagates_mock_error() {
    let mut mock = MockGitHubAppClient::new();
    mock.expect_validate_credentials().times(1).returning(|| {
        Box::pin(async {
            Err(GitHubError::AuthenticationFailed {
                message: String::from("mock authentication failure"),
            })
        })
    });

    let result = validate_with_client(&mock).await;
    assert!(result.is_err(), "expected Err from mock client");
    let message = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("mock authentication failure"),
        "error should propagate mock message: {message}"
    );
}
