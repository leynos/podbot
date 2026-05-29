//! End-to-end behavioural tests for repository cloning.
//!
//! Scenarios drive `clone_repository_into_workspace` against a real Bollard
//! exec client connected to a container started by `testcontainers`, so the
//! production exec seam is exercised against a live container engine rather
//! than a mock. These tests are complementary to the contract-level scenarios
//! in `bdd_repository_cloning.rs`; see `docs/developers-guide.md` §16.1.

mod bdd_repository_cloning_e2e_helpers;

pub use bdd_repository_cloning_e2e_helpers::{
    RepositoryCloningE2eState, repository_cloning_e2e_state,
};
#[expect(
    unused_imports,
    reason = "scenario macro discovers step functions via inventory, but the modules \
              must be reachable from the test crate"
)]
use bdd_repository_cloning_e2e_helpers::{assertions, steps};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/repository_cloning_e2e.feature",
    name = "Repository clone succeeds against a real container"
)]
fn repository_clone_succeeds_against_a_real_container(
    repository_cloning_e2e_state: RepositoryCloningE2eState,
) -> Result<(), String> {
    let _ = repository_cloning_e2e_state;
    Ok(())
}

#[scenario(
    path = "tests/features/repository_cloning_e2e.feature",
    name = "Clone exec failure is reported against a real container"
)]
fn clone_exec_failure_is_reported_against_a_real_container(
    repository_cloning_e2e_state: RepositoryCloningE2eState,
) -> Result<(), String> {
    let _ = repository_cloning_e2e_state;
    Ok(())
}
