//! Behavioural helpers for library boundary scenarios.

mod assertions;
mod state;
mod steps;

pub(crate) type StepResult<T> = Result<T, String>;

#[expect(
    unused_imports,
    reason = "rstest-bdd discovers step functions via attributes, not runtime usage"
)]
pub(crate) use assertions::*;
pub(crate) use state::{LibraryBoundaryState, library_boundary_state};
#[expect(
    unused_imports,
    reason = "rstest-bdd discovers step functions via attributes, not runtime usage"
)]
pub(crate) use steps::*;
