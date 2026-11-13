//! Compile-pass signature lock for the experimental orchestration API.
//!
//! This fixture catches accidental changes to the hosted run, container
//! listing, container stop, and token-daemon boundaries while the surface
//! remains behind the experimental feature gate.

use podbot::api::{
    CommandOutcome, ContainerSummary, DaemonShutdown, RunRequest, list_containers, run_agent,
    run_token_daemon, stop_container,
};
use podbot::config::AppConfig;

fn main() {
    let _run_agent: fn(&AppConfig, &RunRequest) -> podbot::error::Result<CommandOutcome> =
        run_agent;
    let _list_containers: fn(&AppConfig) -> podbot::error::Result<Vec<ContainerSummary>> =
        list_containers;
    let _stop_container: fn(&AppConfig, &str) -> podbot::error::Result<CommandOutcome> =
        stop_container;
    let _run_token_daemon: fn(
        &AppConfig,
        &str,
        DaemonShutdown,
    ) -> podbot::error::Result<CommandOutcome> = run_token_daemon;
}
