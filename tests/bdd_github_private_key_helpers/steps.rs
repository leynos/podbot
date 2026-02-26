//! Given and When step definitions for GitHub private key loading BDD tests.

use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;
use rstest_bdd_macros::{given, when};

use super::state::{GitHubPrivateKeyState, KeyLoadOutcome, StepResult};

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

/// Write `contents` to a file named `key.pem` in a fresh temporary
/// directory, storing the temp dir handle and resolved path in state.
fn write_key_file(
    github_private_key_state: &GitHubPrivateKeyState,
    contents: &str,
) -> StepResult<()> {
    let (tmp, dir, tmp_path) = open_temp_dir()?;
    dir.write("key.pem", contents)
        .map_err(|e| format!("should write key file: {e}"))?;
    github_private_key_state.temp_dir.set(Arc::new(tmp));
    github_private_key_state
        .key_path
        .set(tmp_path.join("key.pem"));
    Ok(())
}

/// Create a temp directory with no key file, storing the path to a
/// non-existent file.
fn set_missing_key_path(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let (tmp, _dir, tmp_path) = open_temp_dir()?;
    github_private_key_state.temp_dir.set(Arc::new(tmp));
    github_private_key_state
        .key_path
        .set(tmp_path.join("nonexistent.pem"));
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

#[given("a public key file exists at the configured path")]
fn public_key_file(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let pem = concat!(
        "-----BEGIN PUBLIC KEY-----\n",
        "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE\n",
        "-----END PUBLIC KEY-----\n"
    );
    write_key_file(github_private_key_state, pem)
}

#[given("a certificate file exists at the configured path")]
fn certificate_file(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let pem = concat!(
        "-----BEGIN CERTIFICATE-----\n",
        "MIICGzCCAaGgAwIBAgIBADAK\n",
        "-----END CERTIFICATE-----\n"
    );
    write_key_file(github_private_key_state, pem)
}

#[given("an OpenSSH private key file exists at the configured path")]
fn openssh_key_file(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let pem = concat!(
        "-----BEGIN OPENSSH PRIVATE KEY-----\n",
        "b3BlbnNzaC1rZXktdjEAAAAA\n",
        "-----END OPENSSH PRIVATE KEY-----\n"
    );
    write_key_file(github_private_key_state, pem)
}

#[given("an encrypted private key file exists at the configured path")]
fn encrypted_key_file(github_private_key_state: &GitHubPrivateKeyState) -> StepResult<()> {
    let pem = concat!(
        "-----BEGIN ENCRYPTED PRIVATE KEY-----\n",
        "MIIFHDBOBgkqhkiG9w0BBQ0w\n",
        "-----END ENCRYPTED PRIVATE KEY-----\n"
    );
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
