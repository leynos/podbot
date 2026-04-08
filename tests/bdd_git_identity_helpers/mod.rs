//! Test helpers for Git identity BDD scenarios.

pub mod assertions;
pub mod state;
pub mod steps;

pub use state::{GitIdentityState, StepResult, git_identity_state};

#[expect(
    unused_imports,
    reason = "step definitions used via rstest-bdd macro expansion"
)]
pub use assertions::*;
#[expect(
    unused_imports,
    reason = "step definitions used via rstest-bdd macro expansion"
)]
pub use steps::*;
