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
use crate::error::Result as PodbotResult;
#[cfg(feature = "experimental")]
use std::time::Duration;
#[cfg(feature = "experimental")]
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
/// - `config.github.validate()` rejects a partial or invalid `GitHub`
///   configuration in [`AppConfig`]
/// - `validate_agent_github_credentials` rejects the configured `app_id` or
///   `private_key_path`
///
/// These validation failures are real runtime behaviour, not placeholder
/// errors deferred until the rest of the orchestration flow is implemented.
#[cfg(feature = "experimental")]
pub fn run_agent(config: &AppConfig, request: &RunRequest) -> PodbotResult<CommandOutcome> {
    debug_run_agent_validation_started(request);
    validate_github_config_for_run(config, request)?;
    validate_configured_github_credentials(config, request)?;
    debug_run_agent_completed(request);
    Ok(CommandOutcome::Success)
}

#[cfg(feature = "experimental")]
fn debug_run_agent_validation_started(request: &RunRequest) {
    tracing::debug!(
        repository = request.repository(),
        branch = request.branch(),
        "validating run request before agent orchestration"
    );
}

#[cfg(feature = "experimental")]
fn debug_run_agent_completed(request: &RunRequest) {
    tracing::debug!(
        operation = "run_agent",
        repository = request.repository(),
        branch = request.branch(),
        outcome = "success",
        "run_agent completed successfully"
    );
}

#[cfg(feature = "experimental")]
fn validate_github_config_for_run(config: &AppConfig, request: &RunRequest) -> PodbotResult<()> {
    if config.github.is_partially_configured() {
        tracing::debug!(
            operation = "run_agent",
            repository = request.repository(),
            branch = request.branch(),
            "validating GitHub configuration for run request"
        );
        let started_at = std::time::Instant::now();
        let result = config.github.validate().inspect_err(|error| {
            warn_github_validation_failed(
                request,
                error,
                None,
                "GitHub configuration validation failed for run request",
            );
        });
        record_github_validation_metrics("config", &result, started_at.elapsed());
        result?;
    }
    Ok(())
}

#[cfg(feature = "experimental")]
fn warn_github_validation_failed(
    request: &RunRequest,
    error: &crate::error::PodbotError,
    app_id: Option<&str>,
    message: &str,
) {
    let app_id_for_log = app_id.unwrap_or("null");
    tracing::warn!(
        operation = "run_agent",
        repository = request.repository(),
        branch = request.branch(),
        app_id = app_id_for_log,
        %error,
        message
    );
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
            operation = "run_agent",
            repository = request.repository(),
            branch = request.branch(),
            app_id,
            "validating GitHub App credentials for run request"
        );
        let started_at = std::time::Instant::now();
        let result =
            validate_agent_github_credentials(app_id, private_key_path).inspect_err(|error| {
                let app_id_text = app_id.to_string();
                warn_github_validation_failed(
                    request,
                    error,
                    Some(app_id_text.as_str()),
                    "GitHub credential authentication failed for run request",
                );
            });
        record_github_validation_metrics("credentials", &result, started_at.elapsed());
        result?;
    }
    Ok(())
}

#[cfg(feature = "experimental")]
fn record_github_validation_metrics(
    validation: &'static str,
    result: &PodbotResult<()>,
    elapsed: Duration,
) {
    let status = if result.is_ok() { "success" } else { "failure" };
    metrics::counter!(
        "podbot.run_agent.github_validation.total",
        "operation" => "run_agent",
        "validation" => validation,
        "status" => status,
    )
    .increment(1);
    metrics::histogram!(
        "podbot.run_agent.github_validation.duration_seconds",
        "operation" => "run_agent",
        "validation" => validation,
        "status" => status,
    )
    .record(elapsed.as_secs_f64());
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
    validate_agent_github_credentials_with(
        app_id,
        private_key_path,
        |validated_app_id, key_path| {
            Box::pin(crate::github::validate_app_credentials(
                validated_app_id,
                key_path,
            ))
        },
    )
}

/// Validate `GitHub` credentials through the execution path required by the
/// current thread.
///
/// Credential validation is safe to call concurrently. Each invocation owns its
/// runtime decision and either creates an isolated local runtime or a scoped
/// helper thread to avoid blocking an existing Tokio runtime.
#[cfg(feature = "experimental")]
fn validate_agent_github_credentials_with<F>(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
    validate: F,
) -> PodbotResult<()>
where
    F: for<'a> Fn(u64, &'a camino::Utf8Path) -> CredentialValidationFuture<'a> + Copy + Sync + Send,
{
    let has_current_runtime = tokio::runtime::Handle::try_current().is_ok();
    debug_github_credential_validation_execution_path(app_id, has_current_runtime);

    if has_current_runtime {
        validate_agent_github_credentials_on_scoped_thread(app_id, private_key_path, validate)
    } else {
        validate_agent_github_credentials_on_local_runtime(app_id, private_key_path, validate)
    }
}

#[cfg(feature = "experimental")]
fn debug_github_credential_validation_execution_path(app_id: u64, has_current_runtime: bool) {
    let redacted_private_key_path = "[REDACTED]";
    let execution_path = github_credential_validation_execution_path(has_current_runtime);
    tracing::debug!(
        app_id,
        private_key_path = %redacted_private_key_path,
        execution_path,
        "selected GitHub credential validation execution path"
    );
}

#[cfg(feature = "experimental")]
const fn github_credential_validation_execution_path(has_current_runtime: bool) -> &'static str {
    if has_current_runtime {
        "scoped-thread"
    } else {
        "local-runtime"
    }
}

/// Validate credentials on a scoped helper thread when already inside Tokio.
///
/// The scoped-thread path keeps nested-runtime calls from blocking the current
/// Tokio worker. Concurrent callers get independent scoped threads and local
/// runtimes; no shared mutable state is retained between invocations.
#[cfg(feature = "experimental")]
fn validate_agent_github_credentials_on_scoped_thread<F>(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
    validate: F,
) -> PodbotResult<()>
where
    F: for<'a> Fn(u64, &'a camino::Utf8Path) -> CredentialValidationFuture<'a> + Copy + Sync + Send,
{
    let redacted_private_key_path = "[REDACTED]";
    tracing::debug!(
        app_id,
        private_key_path = %redacted_private_key_path,
        "validating GitHub App credentials on a scoped thread"
    );
    std::thread::scope(|scope| -> PodbotResult<()> {
        scope
            .spawn(|| {
                validate_agent_github_credentials_on_local_runtime(
                    app_id,
                    private_key_path,
                    validate,
                )
            })
            .join()
            .map_err(|_| credential_validation_thread_panicked())?
    })
}

/// Validate credentials on a fresh local Tokio runtime.
///
/// The local-runtime path builds a fresh runtime for this validation call. It is
/// independent across concurrent invocations and must only be selected when no
/// Tokio runtime is already entered on the current thread.
#[cfg(feature = "experimental")]
fn validate_agent_github_credentials_on_local_runtime<F>(
    app_id: u64,
    private_key_path: &camino::Utf8Path,
    validate: F,
) -> PodbotResult<()>
where
    F: for<'a> Fn(u64, &'a camino::Utf8Path) -> CredentialValidationFuture<'a>,
{
    let redacted_private_key_path = "[REDACTED]";
    tracing::debug!(
        app_id,
        private_key_path = %redacted_private_key_path,
        "validating GitHub App credentials on a local runtime"
    );
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
