//! Orchestration API for podbot commands.
//!
//! This module provides public orchestration functions for each podbot
//! command: [`exec`], [`run_agent`], [`stop_container`], [`list_containers`],
//! and [`run_token_daemon`]. These functions contain the business logic that
//! was previously embedded in the CLI binary, making it available to both
//! the CLI adapter and library embedders.
//!
//! All functions accept library-owned types (not clap types) and return
//! [`crate::error::Result<CommandOutcome>`]. They do not print to
//! stdout/stderr or call `std::process::exit`.

mod exec;

pub use exec::{ExecParams, ExecWithClientParams, exec, exec_with_client};

use crate::config::AppConfig;
use crate::error::Result as PodbotResult;

/// Outcome of a podbot command.
///
/// Commands return either outright success or a command-specific exit code
/// that the CLI adapter maps to a process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcome {
    /// The command completed successfully (exit code 0).
    Success,
    /// The command completed but the underlying process exited with a
    /// non-zero code.
    CommandExit {
        /// The exit code reported by the container engine.
        code: i64,
    },
}

/// Run an AI agent in a sandboxed container.
///
/// Placeholder for the full orchestration flow defined in the design
/// document (steps 1 through 7).
///
/// # Errors
///
/// Will return errors when container orchestration is implemented.
#[expect(
    clippy::missing_const_for_fn,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub is const-eligible but will gain runtime logic"
)]
pub fn run_agent(_config: &AppConfig) -> PodbotResult<CommandOutcome> {
    Ok(CommandOutcome::Success)
}

/// List running podbot containers.
///
/// # Errors
///
/// Will return errors when container listing is implemented.
#[expect(
    clippy::missing_const_for_fn,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub is const-eligible but will gain runtime logic"
)]
pub fn list_containers() -> PodbotResult<CommandOutcome> {
    Ok(CommandOutcome::Success)
}

/// Stop a running container.
///
/// # Errors
///
/// Will return errors when container stop is implemented.
#[expect(
    clippy::missing_const_for_fn,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub is const-eligible but will gain runtime logic"
)]
pub fn stop_container(_container: &str) -> PodbotResult<CommandOutcome> {
    Ok(CommandOutcome::Success)
}

/// Run the token refresh daemon for a container.
///
/// # Errors
///
/// Will return errors when the token daemon is implemented.
#[expect(
    clippy::missing_const_for_fn,
    reason = "FIXME(https://github.com/leynos/podbot/issues/6): stub is const-eligible but will gain runtime logic"
)]
pub fn run_token_daemon(_container_id: &str) -> PodbotResult<CommandOutcome> {
    Ok(CommandOutcome::Success)
}

#[cfg(test)]
mod tests;
