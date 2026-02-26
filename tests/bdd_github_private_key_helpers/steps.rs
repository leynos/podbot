//! Given and When step definitions for GitHub private key loading BDD tests.

use std::sync::Arc;

use camino::Utf8PathBuf;
use rstest_bdd_macros::{given, when};

use super::state::{GitHubPrivateKeyState, KeyLoadOutcome, StepResult};

/// Write `contents` to a file named `key.pem` in a fresh temporary directory,
/// storing the temp dir handle and resolved path in state.
fn write_key_file(
    github_private_key_state: &GitHubPrivateKeyState,
    contents: &str,
) -> StepResult<()> {
    let tmp = tempfile::tempdir().map_err(|e| format!("should create temp dir: {e}"))?;
    std::fs::write(tmp.path().join("key.pem"), contents)
        .map_err(|e| format!("should write key file: {e}"))?;
    let path = Utf8PathBuf::from(
        tmp.path()
            .join("key.pem")
            .to_str()
            .ok_or_else(|| String::from("path should be UTF-8"))?,
    );
    github_private_key_state.temp_dir.set(Arc::new(tmp));
    github_private_key_state.key_path.set(path);
    Ok(())
}

/// Create a temp directory with no key file, storing the path to a
/// non-existent file.
fn set_missing_key_path(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let tmp = tempfile::tempdir().map_err(|e| format!("should create temp dir: {e}"))?;
    let path = Utf8PathBuf::from(
        tmp.path()
            .join("nonexistent.pem")
            .to_str()
            .ok_or_else(|| String::from("path should be UTF-8"))?,
    );
    github_private_key_state.temp_dir.set(Arc::new(tmp));
    github_private_key_state.key_path.set(path);
    Ok(())
}

#[given("a valid RSA private key file exists at the configured path")]
fn valid_rsa_key_file(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let pem = include_str!("../fixtures/test_rsa_private_key.pem");
    write_key_file(github_private_key_state, pem)
}

#[given("no private key file exists at the configured path")]
fn missing_key_file(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    set_missing_key_path(github_private_key_state)
}

#[given("an empty private key file exists at the configured path")]
fn empty_key_file(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    write_key_file(github_private_key_state, "")
}

#[given("a file with invalid PEM content exists at the configured path")]
fn invalid_pem_file(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    write_key_file(github_private_key_state, "this is not a PEM file at all")
}

#[given("an ECDSA private key file exists at the configured path")]
fn ecdsa_key_file(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let pem = include_str!("../fixtures/test_ec_private_key.pem");
    write_key_file(github_private_key_state, pem)
}

#[given("an Ed25519 private key file exists at the configured path")]
fn ed25519_key_file(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let pem = include_str!("../fixtures/test_ed25519_private_key.pem");
    write_key_file(github_private_key_state, pem)
}

#[when("the private key is loaded")]
fn load_private_key(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let key_path = github_private_key_state
        .key_path
        .get()
        .ok_or_else(|| String::from("key path should be set"))?;
    match podbot::github::load_private_key(&key_path) {
        Ok(_) => github_private_key_state
            .outcome
            .set(KeyLoadOutcome::Success),
        Err(error) => github_private_key_state
            .outcome
            .set(KeyLoadOutcome::Failed {
                message: error.to_string(),
            }),
    }
    Ok(())
}
