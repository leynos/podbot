//! Orchestration API for podbot commands.
//!
//! This module provides the stable public orchestration surface: [`exec`],
//! [`ExecContext`], [`ExecRequest`], [`ExecMode`], [`RunRequest`],
//! [`CommandOutcome`], `list_containers`, and `stop_container`. Under
//! `feature = "experimental"`, `run_agent` orchestrates the agent sandbox and
//! `run_token_daemon` refreshes installation tokens for a running container.
//!
//! Internal-feature builds also expose additional compatibility helpers for
//! Git identity configuration.
//!
//! All functions accept library-owned types (not clap types) and return
//! [`crate::error::Result<T>`]. They do not print to stdout/stderr or call
//! `std::process::exit`.

#[cfg(any(feature = "experimental", feature = "internal", test))]
mod configure_git_identity;
#[cfg(any(feature = "experimental", feature = "internal", test))]
mod containers;
mod exec;
mod repository_clone;
mod run;
#[cfg(feature = "experimental")]
mod run_agent;
#[cfg(feature = "experimental")]
mod token_daemon;

#[cfg(feature = "experimental")]
pub use crate::github::DaemonShutdown;
#[cfg(any(feature = "experimental", feature = "internal", test))]
pub use configure_git_identity::{GitIdentityParams, configure_container_git_identity};
#[cfg(any(feature = "experimental", feature = "internal"))]
pub use containers::ContainerSummary;
#[cfg(feature = "experimental")]
pub use containers::{list_containers, stop_container};
#[cfg(feature = "internal")]
#[doc(hidden)]
pub use containers::{list_containers_with_client, stop_container_with_client};
#[cfg(feature = "internal")]
#[doc(hidden)]
pub use exec::exec_with_client_for_tests;
pub use exec::{ExecContext, ExecMode, ExecRequest, exec};
#[cfg(any(feature = "experimental", feature = "internal", test))]
pub use repository_clone::AskpassPath;
pub use repository_clone::{BranchName, RepositoryRef, WorkspacePath};
pub use run::RunRequest;
#[cfg(feature = "experimental")]
pub use run_agent::run_agent;
#[cfg(feature = "experimental")]
pub use token_daemon::run_token_daemon;

use crate::config::AppConfig;
#[cfg(feature = "experimental")]
use crate::error::ConfigError;
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

/// Reject malformed repository and branch inputs for a run request.
/// For example, `owner/name/extra` returns a `run.repository` error.
#[cfg(feature = "experimental")]
fn validate_run_request_for_agent(request: &RunRequest) -> PodbotResult<()> {
    if !is_owner_repository_name(request.repository()) {
        tracing::debug!(
            operation = "run_agent",
            repository = request.repository(),
            branch = request.branch(),
            "run request repository format validation failed"
        );
        return Err(ConfigError::InvalidValue {
            field: String::from("run.repository"),
            reason: String::from("run.repository must use owner/name format"),
        }
        .into());
    }

    if request.branch().chars().any(char::is_whitespace) {
        debug_run_request_branch_validation_failed(request);
        return Err(ConfigError::InvalidValue {
            field: String::from("run.branch"),
            reason: String::from("run.branch must not contain whitespace"),
        }
        .into());
    }

    Ok(())
}

/// Correlate rejected branch whitespace with the originating run request.
/// With a debug-enabled subscriber, a spaced branch emits request-labelled diagnostics.
#[cfg(feature = "experimental")]
fn debug_run_request_branch_validation_failed(request: &RunRequest) {
    tracing::debug!(
        operation = "run_agent",
        repository = request.repository(),
        branch = request.branch(),
        "run request branch whitespace validation failed"
    );
}

/// Require exactly two non-blank segments; `owner/name/extra` is rejected.
#[cfg(feature = "experimental")]
fn is_owner_repository_name(repository: &str) -> bool {
    let mut parts = repository.split('/');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(owner), Some(name), None)
            if is_repository_segment(owner) && is_repository_segment(name)
    )
}

/// Reject empty or whitespace-bearing segments, such as `my owner`.
#[cfg(feature = "experimental")]
fn is_repository_segment(segment: &str) -> bool {
    !segment.is_empty() && !segment.chars().any(char::is_whitespace)
}

/// Require complete `GitHub` settings only when the caller supplies any settings.
/// For example, setting only `app_id` returns an error for the missing settings.
#[cfg(feature = "experimental")]
fn validate_github_config_for_run(config: &AppConfig, request: &RunRequest) -> PodbotResult<()> {
    if config.github.is_partially_configured() {
        debug_github_config_validation_performed(request);
        config.github.validate()?;
    } else {
        debug_github_config_validation_skipped(request);
    }
    Ok(())
}

/// Correlate a required `GitHub` configuration check with its run request.
/// With a debug-enabled subscriber, checking settings emits repository and branch fields.
#[cfg(feature = "experimental")]
fn debug_github_config_validation_performed(request: &RunRequest) {
    tracing::debug!(
        operation = "run_agent",
        repository = request.repository(),
        branch = request.branch(),
        "GitHub configuration validation performed for run request"
    );
}

/// Record that absent `GitHub` settings bypass configuration validation.
/// With a debug-enabled subscriber, absent settings emit a validation-skipped event.
#[cfg(feature = "experimental")]
fn debug_github_config_validation_skipped(request: &RunRequest) {
    tracing::debug!(
        operation = "run_agent",
        repository = request.repository(),
        branch = request.branch(),
        "GitHub configuration validation skipped for run request"
    );
}

/// Resolve and connect an engine client from configuration.
///
/// # Errors
///
/// Returns the same connection errors as [`crate::engine::EngineConnector`].
pub(super) fn connect_engine(config: &AppConfig) -> PodbotResult<bollard::Docker> {
    let env = mockable::DefaultEnv::new();
    let resolver = crate::engine::SocketResolver::new(&env);
    crate::engine::EngineConnector::connect_with_fallback(
        config.engine_socket.as_deref(),
        &resolver,
    )
}

/// Create an isolated runtime and map construction failures into domain errors.
/// For example, failed runtime setup returns `RuntimeCreationFailed`.
fn create_runtime() -> PodbotResult<tokio::runtime::Runtime> {
    tokio::runtime::Runtime::new().map_err(|error| {
        crate::error::PodbotError::from(crate::error::ContainerError::RuntimeCreationFailed {
            message: error.to_string(),
        })
    })
}

#[cfg(test)]
mod tests;
