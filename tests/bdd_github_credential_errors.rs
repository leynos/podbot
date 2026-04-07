//! Behavioural tests for GitHub App credential error classification.
//!
//! These tests validate that podbot classifies different HTTP error
//! responses from the GitHub API into distinct, actionable error
//! messages with remediation hints.

mod bdd_github_credential_errors_helpers;

pub use bdd_github_credential_errors_helpers::{
    GitHubCredentialErrorsState, github_credential_errors_state,
};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/github_credential_errors.feature",
    name = "Credentials rejected by GitHub produce a clear hint"
)]
fn credentials_rejected_by_github_produce_a_clear_hint(
    github_credential_errors_state: GitHubCredentialErrorsState,
) {
    let _ = github_credential_errors_state;
}

#[scenario(
    path = "tests/features/github_credential_errors.feature",
    name = "App not found produces a clear hint"
)]
fn app_not_found_produces_a_clear_hint(
    github_credential_errors_state: GitHubCredentialErrorsState,
) {
    let _ = github_credential_errors_state;
}

#[scenario(
    path = "tests/features/github_credential_errors.feature",
    name = "GitHub server error produces a retry hint"
)]
fn github_server_error_produces_a_retry_hint(
    github_credential_errors_state: GitHubCredentialErrorsState,
) {
    let _ = github_credential_errors_state;
}

#[scenario(
    path = "tests/features/github_credential_errors.feature",
    name = "Permission error produces a permissions hint"
)]
fn permission_error_produces_a_permissions_hint(
    github_credential_errors_state: GitHubCredentialErrorsState,
) {
    let _ = github_credential_errors_state;
}
