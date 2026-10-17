//! Behavioural helpers for interactive execution scenarios.

mod assertions;
mod state;
pub(crate) mod stdin_forwarding;
mod steps;

#[expect(
    unused_imports,
    reason = "rstest-bdd discovers step functions via attributes, not runtime usage"
)]
pub(crate) use assertions::*;
pub(crate) use state::{InteractiveExecState, interactive_exec_state};
#[expect(
    unused_imports,
    reason = "rstest-bdd discovers step functions via attributes, not runtime usage"
)]
pub(crate) use steps::*;
