//! Shared state and step definitions for library boundary BDD scenarios.
//!
//! Private step and assertion modules contain annotated steps registered by
//! `rstest-bdd`. This module re-exports the state and fixture used by the
//! scenario tests.

mod assertions;
mod state;
mod steps;

pub(crate) type StepResult<T> = Result<T, String>;

pub(crate) use state::{LibraryBoundaryState, library_boundary_state};
