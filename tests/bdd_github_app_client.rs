//! Behavioural tests for GitHub App client construction.
//!
//! These tests validate that podbot correctly builds an authenticated
//! Octocrab client from App credentials.

mod bdd_github_app_client_helpers;

pub use bdd_github_app_client_helpers::{GitHubAppClientState, github_app_client_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/github_app_client.feature",
    name = "Valid credentials produce an App client"
)]
fn valid_credentials_produce_an_app_client(github_app_client_state: GitHubAppClientState) {
    let _ = github_app_client_state;
}

#[scenario(
    path = "tests/features/github_app_client.feature",
    name = "Zero App ID is accepted by the builder"
)]
fn zero_app_id_is_accepted_by_the_builder(github_app_client_state: GitHubAppClientState) {
    let _ = github_app_client_state;
}
