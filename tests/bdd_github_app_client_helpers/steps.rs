//! Given and When step definitions for GitHub App client construction
//! BDD tests.

use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;
use rstest_bdd_macros::{given, when};

use super::state::{ClientBuildOutcome, GitHubAppClientState, StepResult};

/// Open a temporary directory as a `cap_std` capability handle and return
/// both the `TempDir` guard and a UTF-8 path to it.
fn open_temp_dir() -> StepResult<(tempfile::TempDir, Dir, Utf8PathBuf)> {
    let tmp = tempfile::tempdir().map_err(|e| format!("should create temp dir: {e}"))?;
    let tmp_path = Utf8Path::from_path(tmp.path())
        .ok_or_else(|| String::from("temp dir path should be UTF-8"))?
        .to_owned();
    let dir = Dir::open_ambient_dir(&tmp_path, ambient_authority())
        .map_err(|e| format!("should open temp dir: {e}"))?;
    Ok((tmp, dir, tmp_path))
}

#[given("a valid RSA private key file exists at the configured path")]
fn valid_rsa_key_file(github_app_client_state: &GitHubAppClientState) -> StepResult<()> {
    let pem = include_str!("../fixtures/test_rsa_private_key.pem");
    let (tmp, dir, tmp_path) = open_temp_dir()?;
    dir.write("key.pem", pem)
        .map_err(|e| format!("should write key file: {e}"))?;
    github_app_client_state.temp_dir.set(Arc::new(tmp));
    github_app_client_state
        .key_path
        .set(tmp_path.join("key.pem"));
    Ok(())
}

#[given("the GitHub App ID is {app_id}")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn set_app_id(github_app_client_state: &GitHubAppClientState, app_id: u64) -> StepResult<()> {
    github_app_client_state.app_id.set(app_id);
    Ok(())
}

#[when("the App client is built")]
fn build_client(github_app_client_state: &GitHubAppClientState) -> StepResult<()> {
    let key_path = github_app_client_state
        .key_path
        .get()
        .ok_or_else(|| String::from("key path should be set"))?;
    let app_id = github_app_client_state
        .app_id
        .get()
        .ok_or_else(|| String::from("app_id should be set"))?;

    let key =
        podbot::github::load_private_key(&key_path).map_err(|e| format!("key load failed: {e}"))?;

    // Octocrab's build() requires a Tokio runtime for Tower buffer tasks.
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;
    let _guard = rt.enter();

    match podbot::github::build_app_client(app_id, key) {
        Ok(_) => github_app_client_state
            .outcome
            .set(ClientBuildOutcome::Success),
        Err(error) => github_app_client_state
            .outcome
            .set(ClientBuildOutcome::Failed {
                message: error.to_string(),
            }),
    }
    Ok(())
}
