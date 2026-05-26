//! Behavioural helpers for orchestration BDD scenarios.
//!
//! This module gathers the shared assertions, state, and step bindings used by
//! the orchestration feature tests so the scenarios stay focused on outcomes
//! rather than setup plumbing.
//! It re-exports the helper modules consumed by `rstest-bdd` discovery and
//! keeps the orchestration scenarios tied to the public API surface under
//! test.

mod assertions;
mod state;
mod steps;

pub(crate) type StepResult<T> = Result<T, String>;

#[expect(
    unused_imports,
    reason = "rstest-bdd discovers step functions via attributes, not runtime usage"
)]
pub(crate) use assertions::*;
pub(crate) use state::{OrchestrationState, orchestration_state};
#[expect(
    unused_imports,
    reason = "rstest-bdd discovers step functions via attributes, not runtime usage"
)]
pub(crate) use steps::*;
