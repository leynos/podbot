//! Behavioural tests for repository cloning.

mod bdd_repository_cloning_helpers;

pub use bdd_repository_cloning_helpers::{RepositoryCloningState, repository_cloning_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/repository_cloning.feature",
    name = "Repository clone succeeds"
)]
fn repository_clone_succeeds(repository_cloning_state: RepositoryCloningState) {
    let _ = repository_cloning_state;
}

#[scenario(
    path = "tests/features/repository_cloning.feature",
    name = "Malformed repository input fails before exec"
)]
fn malformed_repository_input_fails_before_exec(repository_cloning_state: RepositoryCloningState) {
    let _ = repository_cloning_state;
}

#[scenario(
    path = "tests/features/repository_cloning.feature",
    name = "Clone exec failure is reported"
)]
fn clone_exec_failure_is_reported(repository_cloning_state: RepositoryCloningState) {
    let _ = repository_cloning_state;
}

#[scenario(
    path = "tests/features/repository_cloning.feature",
    name = "Branch verification failure is reported"
)]
fn branch_verification_failure_is_reported(repository_cloning_state: RepositoryCloningState) {
    let _ = repository_cloning_state;
}
