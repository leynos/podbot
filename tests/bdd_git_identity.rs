//! Behavioural tests for Git identity configuration.

mod bdd_git_identity_helpers;

pub use bdd_git_identity_helpers::{GitIdentityState, git_identity_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "Both name and email configured on host"
)]
fn both_name_and_email_configured(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "Only user name configured on host"
)]
fn only_user_name_configured(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "Only user email configured on host"
)]
fn only_user_email_configured(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "No Git identity configured on host"
)]
fn no_git_identity_configured(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "Container exec fails for one field"
)]
fn container_exec_fails_for_one_field(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}
