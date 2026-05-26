//! Behavioural tests for GitHub App installation-token acquisition.

#![cfg(feature = "internal")]

mod bdd_github_installation_token_helpers;

pub use bdd_github_installation_token_helpers::{
    GitHubInstallationTokenState, github_installation_token_state,
};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/github_installation_token.feature",
    name = "Installation token acquisition succeeds"
)]
fn installation_token_acquisition_succeeds(
    github_installation_token_state: GitHubInstallationTokenState,
) {
    let _ = github_installation_token_state;
}

#[scenario(
    path = "tests/features/github_installation_token.feature",
    name = "Installation token acquisition fails semantically"
)]
fn installation_token_acquisition_fails_semantically(
    github_installation_token_state: GitHubInstallationTokenState,
) {
    let _ = github_installation_token_state;
}
