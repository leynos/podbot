//! Behavioural tests for GitHub installation-token acquisition.
//!
//! These tests validate the observable outcomes for expiry buffering and
//! GitHub API failure handling without making live network calls.

mod bdd_github_installation_token_helpers;

pub use bdd_github_installation_token_helpers::{
    GitHubInstallationTokenState, github_installation_token_state,
};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/github_installation_token.feature",
    name = "Valid credentials produce an installation token"
)]
fn valid_credentials_produce_an_installation_token(
    github_installation_token_state: GitHubInstallationTokenState,
) {
    std::mem::drop(github_installation_token_state);
}

#[scenario(
    path = "tests/features/github_installation_token.feature",
    name = "Token expiry inside the buffer is rejected"
)]
fn token_expiry_inside_the_buffer_is_rejected(
    github_installation_token_state: GitHubInstallationTokenState,
) {
    std::mem::drop(github_installation_token_state);
}

#[scenario(
    path = "tests/features/github_installation_token.feature",
    name = "GitHub rejects installation token acquisition"
)]
fn github_rejects_installation_token_acquisition(
    github_installation_token_state: GitHubInstallationTokenState,
) {
    std::mem::drop(github_installation_token_state);
}

#[scenario(
    path = "tests/features/github_installation_token.feature",
    name = "Missing expiry metadata is rejected"
)]
fn missing_expiry_metadata_is_rejected(
    github_installation_token_state: GitHubInstallationTokenState,
) {
    std::mem::drop(github_installation_token_state);
}
