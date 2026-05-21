//! Orchestration API for podbot commands.
//!
//! This module provides the stable public exec orchestration surface:
//! [`exec`], [`ExecContext`], [`ExecRequest`], [`ExecMode`], and
//! [`CommandOutcome`]. Under `feature = "experimental"`, `run_agent`
//! performs `GitHub` configuration and credential validation, while
//! `stop_container`, `list_containers`, and `run_token_daemon` remain
//! compatibility stubs.
//!
//! Internal-feature builds also expose additional compatibility helpers for
//! Git identity configuration.
//!
//! All functions accept library-owned types (not clap types) and return
//! [`crate::error::Result<T>`]. They do not print to stdout/stderr or call
//! `std::process::exit`.

#[cfg(any(feature = "internal", test))]
mod configure_git_identity;
mod exec;
mod run;

#[cfg(any(feature = "internal", test))]
pub use configure_git_identity::{GitIdentityParams, configure_container_git_identity};
#[cfg(feature = "internal")]
#[doc(hidden)]
pub use exec::exec_with_client_for_tests;
pub use exec::{ExecContext, ExecMode, ExecRequest, exec};
pub use run::RunRequest;

#[cfg(feature = "experimental")]
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
#[cfg(feature = "experimental")]
pub fn run_agent(config: &AppConfig, request: &RunRequest) -> PodbotResult<CommandOutcome> {
    tracing::debug!(
        repository = request.repository(),
        branch = request.branch(),
        "validating run request before agent orchestration"
    );
    validate_github_config_for_run(config, request)?;
    validate_configured_github_credentials(config, request)?;
    Ok(CommandOutcome::Success)
}

#[cfg(feature = "experimental")]
fn validate_github_config_for_run(config: &AppConfig, request: &RunRequest) -> PodbotResult<()> {
    if config.github.is_partially_configured() {
        tracing::debug!(
            repository = request.repository(),
            branch = request.branch(),
            "validating GitHub configuration for run request"
        );
        config.github.validate()?;
    }
    Ok(())
}

#[cfg(feature = "experimental")]
fn validate_configured_github_credentials(
    config: &AppConfig,
    request: &RunRequest,
) -> PodbotResult<()> {
    if let (Some(app_id), Some(private_key_path)) = (
        config.github.app_id,
        config.github.private_key_path.as_ref(),
    ) {
        tracing::debug!(
            repository = request.repository(),
            branch = request.branch(),
            app_id,
            "validating GitHub App credentials for run request"
        );
        validate_agent_github_credentials(app_id, private_key_path)?;
    }
    Ok(())
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
#[cfg(feature = "experimental")]
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
#[cfg(feature = "experimental")]
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
#[cfg(feature = "experimental")]
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

#[cfg(feature = "experimental")]
fn validate_agent_github_credentials(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
) -> PodbotResult<()> {
    if tokio::runtime::Handle::try_current().is_ok() {
        validate_agent_github_credentials_on_scoped_thread(app_id, private_key_path)
    } else {
        validate_agent_github_credentials_on_local_runtime(app_id, private_key_path)
    }
}

#[cfg(feature = "experimental")]
fn validate_agent_github_credentials_on_scoped_thread(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
) -> PodbotResult<()> {
    let redacted_private_key_path = "[REDACTED]";
    tracing::debug!(
        app_id,
        private_key_path = %redacted_private_key_path,
        "validating GitHub App credentials on a scoped thread"
    );
    std::thread::scope(|scope| -> PodbotResult<()> {
        scope
            .spawn(|| validate_agent_github_credentials_on_local_runtime(app_id, private_key_path))
            .join()
            .map_err(|_| credential_validation_thread_panicked())?
    })
}

#[cfg(feature = "experimental")]
fn validate_agent_github_credentials_on_local_runtime(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
) -> PodbotResult<()> {
    let redacted_private_key_path = "[REDACTED]";
    tracing::debug!(
        app_id,
        private_key_path = %redacted_private_key_path,
        "validating GitHub App credentials on a local runtime"
    );
    let runtime = create_runtime()?;
    runtime
        .block_on(crate::github::validate_app_credentials(
            app_id,
            private_key_path,
        ))
        .map_err(crate::error::PodbotError::from)
}

#[cfg(feature = "experimental")]
fn credential_validation_thread_panicked() -> crate::error::PodbotError {
    crate::error::PodbotError::from(crate::error::GitHubError::AuthenticationFailed {
        message: String::from("GitHub credential validation thread panicked"),
    })
}

#[cfg(test)]
mod tests;
