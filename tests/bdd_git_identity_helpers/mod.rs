//! Test helpers for Git identity BDD scenarios.

pub mod assertions;
pub mod state;
pub mod steps;
pub mod test_helpers;

pub use state::{GitIdentityState, git_identity_state};

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
