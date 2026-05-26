//! Orchestration API for podbot commands.
//!
//! This module provides the stable public orchestration surface: [`exec`],
//! [`ExecContext`], [`ExecRequest`], [`ExecMode`], [`RunRequest`], and
//! [`CommandOutcome`]. Under `feature = "experimental"`, `run_agent` performs
//! `GitHub` configuration and credential validation, while `stop_container`,
//! `list_containers`, and `run_token_daemon` remain compatibility stubs.
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
#[cfg(feature = "experimental")]
use crate::error::ConfigError;
use crate::error::Result as PodbotResult;
#[cfg(all(feature = "experimental", test))]
type CredentialValidationFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<(), crate::error::GitHubError>> + 'a>,
>;

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
/// - `request` does not identify a repository in `owner/name` format or uses
///   a branch name containing whitespace
/// - `config.github.validate()` rejects a partial or invalid `GitHub`
///   configuration in [`AppConfig`]
/// - `validate_agent_github_credentials` rejects the configured `app_id` or
///   `private_key_path`
///
/// These validation failures are real runtime behaviour, not placeholder
/// errors deferred until the rest of the orchestration flow is implemented.
#[cfg(feature = "experimental")]
pub fn run_agent(config: &AppConfig, request: &RunRequest) -> PodbotResult<CommandOutcome> {
    validate_run_request_for_agent(request)?;
    validate_github_config_for_run(config, request)?;
    validate_configured_github_credentials(config, request)?;
    Ok(CommandOutcome::Success)
}

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

#[cfg(feature = "experimental")]
fn debug_run_request_branch_validation_failed(request: &RunRequest) {
    tracing::debug!(
        operation = "run_agent",
        repository = request.repository(),
        branch = request.branch(),
        "run request branch whitespace validation failed"
    );
}

#[cfg(feature = "experimental")]
fn is_owner_repository_name(repository: &str) -> bool {
    let mut parts = repository.split('/');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(owner), Some(name), None)
            if is_repository_segment(owner) && is_repository_segment(name)
    )
}

#[cfg(feature = "experimental")]
fn is_repository_segment(segment: &str) -> bool {
    !segment.is_empty() && !segment.chars().any(char::is_whitespace)
}

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

#[cfg(feature = "experimental")]
fn validate_configured_github_credentials(
    config: &AppConfig,
    request: &RunRequest,
) -> PodbotResult<()> {
    if let (Some(app_id), Some(private_key_path)) = (
        config.github.app_id,
        config.github.private_key_path.as_ref(),
    ) {
        debug_github_credential_validation_performed(request, app_id);
        validate_agent_github_credentials(app_id, private_key_path)?;
    } else {
        debug_github_credential_validation_skipped(request);
    }
    Ok(())
}

#[cfg(feature = "experimental")]
fn debug_github_config_validation_performed(request: &RunRequest) {
    tracing::debug!(
        operation = "run_agent",
        repository = request.repository(),
        branch = request.branch(),
        "GitHub configuration validation performed for run request"
    );
}

#[cfg(feature = "experimental")]
fn debug_github_config_validation_skipped(request: &RunRequest) {
    tracing::debug!(
        operation = "run_agent",
        repository = request.repository(),
        branch = request.branch(),
        "GitHub configuration validation skipped for run request"
    );
}

#[cfg(feature = "experimental")]
fn debug_github_credential_validation_performed(request: &RunRequest, app_id: u64) {
    tracing::debug!(
        operation = "run_agent",
        repository = request.repository(),
        branch = request.branch(),
        app_id,
        "GitHub credential validation performed for run request"
    );
}

#[cfg(feature = "experimental")]
fn debug_github_credential_validation_skipped(request: &RunRequest) {
    tracing::debug!(
        operation = "run_agent",
        repository = request.repository(),
        branch = request.branch(),
        "GitHub credential validation skipped for run request"
    );
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
    validate_agent_github_credentials_on_scoped_thread(app_id, private_key_path)
}

#[cfg(all(feature = "experimental", test))]
fn validate_agent_github_credentials_with<F>(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
    validate: F,
) -> PodbotResult<()>
where
    F: for<'a> Fn(u64, &'a camino::Utf8Path) -> CredentialValidationFuture<'a> + Sync + Send,
{
    validate_agent_github_credentials_on_scoped_thread_with(app_id, private_key_path, validate)
}

/// Validate credentials on a scoped helper thread.
///
/// The helper thread keeps runtime creation out of the caller's thread.
/// Concurrent callers get independent scoped threads and local runtimes; no
/// shared mutable state is retained between invocations.
#[cfg(feature = "experimental")]
fn validate_agent_github_credentials_on_scoped_thread(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
) -> PodbotResult<()> {
    std::thread::scope(|scope| -> PodbotResult<()> {
        scope
            .spawn(|| validate_agent_github_credentials_on_local_runtime(app_id, private_key_path))
            .join()
            .map_err(|_| credential_validation_thread_panicked())?
    })
}

#[cfg(all(feature = "experimental", test))]
fn validate_agent_github_credentials_on_scoped_thread_with<F>(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
    validate: F,
) -> PodbotResult<()>
where
    F: for<'a> Fn(u64, &'a camino::Utf8Path) -> CredentialValidationFuture<'a> + Sync + Send,
{
    std::thread::scope(|scope| -> PodbotResult<()> {
        scope
            .spawn(|| {
                validate_agent_github_credentials_on_local_runtime_with(
                    app_id,
                    private_key_path,
                    validate,
                )
            })
            .join()
            .map_err(|_| credential_validation_thread_panicked())?
    })
}

#[cfg(feature = "experimental")]
fn validate_agent_github_credentials_on_local_runtime(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
) -> PodbotResult<()> {
    let runtime = create_runtime()?;
    runtime
        .block_on(crate::github::validate_app_credentials(
            app_id,
            private_key_path,
        ))
        .map_err(crate::error::PodbotError::from)
}

#[cfg(all(feature = "experimental", test))]
fn validate_agent_github_credentials_on_local_runtime_with<F>(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
    validate: F,
) -> PodbotResult<()>
where
    F: for<'a> Fn(u64, &'a camino::Utf8Path) -> CredentialValidationFuture<'a>,
{
    let runtime = create_runtime()?;
    runtime
        .block_on(validate(app_id, private_key_path))
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
