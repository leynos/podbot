//! Shared state and step definitions for orchestration BDD scenarios.
//!
//! Private step/assertion modules contain definitions registered by `rstest-bdd`
//! attributes. Scenario-facing state and its fixture are re-exported here.

mod assertions;
mod state;
mod steps;

pub(crate) type StepResult<T> = Result<T, String>;

pub(crate) use state::{OrchestrationState, orchestration_state};
