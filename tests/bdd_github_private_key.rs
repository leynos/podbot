//! Behavioural tests for GitHub App private key loading.
//!
//! These tests validate that podbot correctly loads RSA private keys
//! and rejects unsupported key types with clear error messages.

mod bdd_github_private_key_helpers;

pub use bdd_github_private_key_helpers::{GitHubPrivateKeyState, github_private_key_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/github_private_key.feature",
    name = "Valid RSA private key is loaded successfully"
)]
fn valid_rsa_private_key_is_loaded_successfully(github_private_key_state: GitHubPrivateKeyState) {
    let _ = github_private_key_state;
}

#[scenario(
    path = "tests/features/github_private_key.feature",
    name = "Missing key file produces a clear error"
)]
fn missing_key_file_produces_a_clear_error(github_private_key_state: GitHubPrivateKeyState) {
    let _ = github_private_key_state;
}

#[scenario(
    path = "tests/features/github_private_key.feature",
    name = "Empty key file produces a clear error"
)]
fn empty_key_file_produces_a_clear_error(github_private_key_state: GitHubPrivateKeyState) {
    let _ = github_private_key_state;
}

#[scenario(
    path = "tests/features/github_private_key.feature",
    name = "Invalid PEM content produces a clear error"
)]
fn invalid_pem_content_produces_a_clear_error(github_private_key_state: GitHubPrivateKeyState) {
    let _ = github_private_key_state;
}

#[scenario(
    path = "tests/features/github_private_key.feature",
    name = "ECDSA key is rejected with a clear error"
)]
fn ecdsa_key_is_rejected_with_a_clear_error(github_private_key_state: GitHubPrivateKeyState) {
    let _ = github_private_key_state;
}

#[scenario(
    path = "tests/features/github_private_key.feature",
    name = "Ed25519 key is rejected with a clear error"
)]
fn ed25519_key_is_rejected_with_a_clear_error(github_private_key_state: GitHubPrivateKeyState) {
    let _ = github_private_key_state;
}
