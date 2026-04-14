//! Orchestration API for podbot commands.
//!
//! This module provides the stable public orchestration functions for each
//! supported command: [`exec`], [`run_agent`], [`stop_container`],
//! [`list_containers`], [`run_token_daemon`], and
//! [`configure_container_git_identity`]. These functions contain the business
//! logic that was previously embedded in the CLI binary, making it available to
//! both the CLI adapter and library embedders.
//!
//! All functions accept library-owned types (not clap types) and return
//! [`crate::error::Result<T>`]. They do not print to stdout/stderr or call
//! `std::process::exit`.

mod configure_git_identity;
mod exec;

pub use configure_git_identity::{GitIdentityParams, configure_container_git_identity};
pub use exec::{ExecContext, ExecMode, ExecRequest, exec};
/// Advanced exec helper for callers that already manage engine connections.
///
/// This function exists for low-level embedders and test harnesses that need
/// to inject or reuse a [`crate::engine::ContainerExecClient`]. Because it
/// depends directly on the engine trait surface, it is more coupled to
/// internals than the small stable embedding boundary centred on
/// [`ExecRequest`], [`ExecContext`], and [`exec`].
#[doc(hidden)]
pub use exec::exec_with_client;

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
/// This orchestration entry point currently performs `GitHub` configuration
/// and credential validation before the wider agent lifecycle exists. If any
/// `GitHub` field is present in the supplied [`AppConfig`], `run_agent` calls
/// `config.github.validate()` immediately to require a complete credential
/// set. When both `app_id` and `private_key_path` are present, it then calls
/// `validate_agent_github_credentials` to confirm the key material can
/// authenticate successfully.
///
/// # Errors
///
/// Returns errors immediately when:
/// - `config.github.validate()` rejects a partial or invalid `GitHub`
///   configuration in [`AppConfig`]
/// - `validate_agent_github_credentials` rejects the configured `app_id` or
///   `private_key_path`
///
/// These validation failures are real runtime behaviour, not placeholder
/// errors deferred until the rest of the orchestration flow is implemented.
pub fn run_agent(config: &AppConfig) -> PodbotResult<CommandOutcome> {
    if config.github.is_partially_configured() {
        config.github.validate()?;
    }

    if let (Some(app_id), Some(private_key_path)) = (
        config.github.app_id,
        config.github.private_key_path.as_ref(),
    ) {
        validate_agent_github_credentials(app_id, private_key_path)?;
    }
    Ok(CommandOutcome::Success)
}

/// List running podbot containers.
///
/// # Errors
///
/// Will return errors when container listing is implemented.
#[expect(
    clippy::missing_const_for_fn,
    reason = "FIXME(https://github.com/leynos/podbot/issues/51): stub is const-eligible but will gain runtime logic"
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
    reason = "FIXME(https://github.com/leynos/podbot/issues/51): stub is const-eligible but will gain runtime logic"
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
    reason = "FIXME(https://github.com/leynos/podbot/issues/51): stub is const-eligible but will gain runtime logic"
)]
pub fn run_token_daemon(_container_id: &str) -> PodbotResult<CommandOutcome> {
    Ok(CommandOutcome::Success)
}

fn create_runtime() -> PodbotResult<tokio::runtime::Runtime> {
    tokio::runtime::Runtime::new().map_err(|error| {
        crate::error::PodbotError::from(crate::error::ContainerError::RuntimeCreationFailed {
            message: error.to_string(),
        })
    })
}

fn validate_agent_github_credentials(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
) -> PodbotResult<()> {
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|scope| -> PodbotResult<()> {
            scope
                .spawn(|| -> PodbotResult<()> {
                    let runtime = create_runtime()?;
                    runtime
                        .block_on(crate::github::validate_app_credentials(
                            app_id,
                            private_key_path,
                        ))
                        .map_err(crate::error::PodbotError::from)
                })
                .join()
                .map_err(|_| credential_validation_thread_panicked())?
        })
    } else {
        let runtime = create_runtime()?;
        runtime
            .block_on(crate::github::validate_app_credentials(
                app_id,
                private_key_path,
            ))
            .map_err(crate::error::PodbotError::from)
    }
}

fn credential_validation_thread_panicked() -> crate::error::PodbotError {
    crate::error::PodbotError::from(crate::error::GitHubError::AuthenticationFailed {
        message: String::from("GitHub credential validation thread panicked"),
    })
}

#[cfg(test)]
mod tests;
