//! Behavioural tests for Git identity configuration.

#![cfg(feature = "internal")]

mod bdd_git_identity_helpers;

pub use bdd_git_identity_helpers::{GitIdentityState, git_identity_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "Both name and email are configured"
)]
fn both_name_and_email_are_configured(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "Only name is configured on the host"
)]
fn only_name_is_configured_on_the_host(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "Only email is configured on the host"
)]
fn only_email_is_configured_on_the_host(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "Neither name nor email is configured"
)]
fn neither_name_nor_email_is_configured(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "Multi-word name is configured"
)]
fn multi_word_name_is_configured(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}

#[scenario(
    path = "tests/features/git_identity.feature",
    name = "Container exec failure propagates as error"
)]
fn container_exec_failure_propagates_as_error(git_identity_state: GitIdentityState) {
    let _ = git_identity_state;
}
