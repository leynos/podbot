//! Compile-pass signature lock for the experimental `run_agent` API.
//!
//! This fixture catches accidental removal of the `RunRequest` parameter from
//! the hosted run boundary while the surface remains behind the experimental
//! feature gate.

use podbot::api::{CommandOutcome, RunRequest, run_agent};
use podbot::config::AppConfig;

fn main() {
    let _run_agent: fn(&AppConfig, &RunRequest) -> podbot::error::Result<CommandOutcome> =
        run_agent;
}
