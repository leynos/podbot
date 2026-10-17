//! Behavioural helpers for interactive execution scenarios.

mod assertions;
mod state;
pub(crate) mod stdin_forwarding;
mod steps;

pub(crate) use state::{InteractiveExecState, interactive_exec_state};
