//! Behavioural tests for library boundary stability.
//!
//! These BDD scenarios verify that Podbot can be embedded as a library
//! dependency with a self-contained API surface. Each scenario exercises the
//! public library boundary from a host-application perspective — loading
//! configuration without CLI types, executing commands via the orchestration
//! API, receiving semantic errors, and calling stub orchestration functions.
//!
//! Scenarios are defined in `tests/features/library_boundary.feature` and
//! step definitions live in the `bdd_library_boundary_helpers` module.

#![cfg(feature = "internal")]

mod bdd_library_boundary_helpers;
mod test_utils;

use bdd_library_boundary_helpers::{LibraryBoundaryState, library_boundary_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/library_boundary.feature",
    name = "Library consumer loads configuration without CLI types"
)]
fn library_loads_config(library_boundary_state: LibraryBoundaryState) {
    let _ = library_boundary_state;
}

#[scenario(
    path = "tests/features/library_boundary.feature",
    name = "Library consumer executes a command via the API"
)]
fn library_exec_command(library_boundary_state: LibraryBoundaryState) {
    let _ = library_boundary_state;
}

#[scenario(
    path = "tests/features/library_boundary.feature",
    name = "Library consumer receives semantic error for exec failure"
)]
fn library_exec_failure(library_boundary_state: LibraryBoundaryState) {
    let _ = library_boundary_state;
}

#[scenario(
    path = "tests/features/library_boundary.feature",
    name = "Stub orchestration functions return success"
)]
#[cfg(feature = "experimental")]
fn stub_functions_succeed(library_boundary_state: LibraryBoundaryState) {
    let _ = library_boundary_state;
}
