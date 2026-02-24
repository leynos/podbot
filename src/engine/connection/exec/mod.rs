//! Container exec lifecycle with attached/detached modes and terminal handling.
//!
//! This module wraps Bollard exec APIs behind a small trait seam so command
//! execution behaviour can be unit-tested without a live daemon.

mod attached;
mod terminal;

use std::future::Future;
use std::pin::Pin;

use bollard::exec::{CreateExecOptions, CreateExecResults, ResizeExecOptions, StartExecOptions};
use bollard::{Docker, errors::Error as BollardError};

use self::attached::{run_attached_session_async, wait_for_exit_code_async};
use self::terminal::{SystemTerminalSizeProvider, TerminalSizeProvider};
use super::EngineConnector;
use crate::error::{ConfigError, ContainerError, PodbotError};

pub(super) const EXEC_INSPECT_POLL_INTERVAL_MS: u64 = 100;

/// Boxed future type returned by [`ContainerExecClient::create_exec`].
pub type CreateExecFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CreateExecResults, BollardError>> + Send + 'a>>;

/// Boxed future type returned by [`ContainerExecClient::start_exec`].
pub type StartExecFuture<'a> = Pin<
    Box<dyn Future<Output = Result<bollard::exec::StartExecResults, BollardError>> + Send + 'a>,
>;

/// Boxed future type returned by [`ContainerExecClient::inspect_exec`].
pub type InspectExecFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<bollard::models::ExecInspectResponse, BollardError>> + Send + 'a,
    >,
>;

/// Boxed future type returned by [`ContainerExecClient::resize_exec`].
pub type ResizeExecFuture<'a> = Pin<Box<dyn Future<Output = Result<(), BollardError>> + Send + 'a>>;

/// Behaviour required to run and inspect exec sessions.
///
/// This abstraction keeps command execution testable without a live daemon.
pub trait ContainerExecClient {
    /// Create an exec session in a running container.
    fn create_exec(
        &self,
        container_id: &str,
        options: CreateExecOptions<String>,
    ) -> CreateExecFuture<'_>;

    /// Start a previously created exec session.
    fn start_exec(&self, exec_id: &str, options: Option<StartExecOptions>) -> StartExecFuture<'_>;

    /// Inspect an exec session for running status and exit code.
    fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;

    /// Resize a running exec pseudo-terminal.
    fn resize_exec(&self, exec_id: &str, options: ResizeExecOptions) -> ResizeExecFuture<'_>;
}

impl ContainerExecClient for Docker {
    fn create_exec(
        &self,
        container_id: &str,
        options: CreateExecOptions<String>,
    ) -> CreateExecFuture<'_> {
        let container_id_owned = String::from(container_id);
        Box::pin(async move { Self::create_exec(self, &container_id_owned, options).await })
    }

    fn start_exec(&self, exec_id: &str, options: Option<StartExecOptions>) -> StartExecFuture<'_> {
        let exec_id_owned = String::from(exec_id);
        Box::pin(async move { Self::start_exec(self, &exec_id_owned, options).await })
    }

    fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_> {
        let exec_id_owned = String::from(exec_id);
        Box::pin(async move { Self::inspect_exec(self, &exec_id_owned).await })
    }

    fn resize_exec(&self, exec_id: &str, options: ResizeExecOptions) -> ResizeExecFuture<'_> {
        let exec_id_owned = String::from(exec_id);
        Box::pin(async move { Self::resize_exec(self, &exec_id_owned, options).await })
    }
}

/// Execution mode for container commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecMode {
    /// Attach local terminal streams to the exec process.
    Attached,
    /// Start without stream attachment and wait for exit.
    Detached,
}

impl ExecMode {
    #[must_use]
    const fn is_attached(self) -> bool {
        matches!(self, Self::Attached)
    }
}

/// Parameters required to run a command in a running container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecRequest {
    container_id: String,
    command: Vec<String>,
    env: Option<Vec<String>>,
    mode: ExecMode,
    tty: bool,
}

impl ExecRequest {
    /// Create a new command execution request.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` when `container_id` or `command`
    /// is empty.
    pub fn new(
        container_id: impl Into<String>,
        command: Vec<String>,
        mode: ExecMode,
    ) -> Result<Self, PodbotError> {
        let container_id_value = container_id.into();
        let id = String::from(validate_required_field("container", &container_id_value)?);
        let validated_command = validate_command(command)?;

        Ok(Self {
            container_id: id,
            command: validated_command,
            env: None,
            mode,
            tty: mode.is_attached(),
        })
    }

    /// Set environment variables in `KEY=value` form.
    #[must_use]
    pub fn with_env(mut self, env: Option<Vec<String>>) -> Self {
        self.env = env.filter(|entries| !entries.is_empty());
        self
    }

    /// Control pseudo-terminal allocation for attached mode.
    ///
    /// Detached mode always forces `tty = false`.
    #[must_use]
    pub const fn with_tty(mut self, tty: bool) -> Self {
        self.tty = self.mode.is_attached() && tty;
        self
    }

    /// Return target container identifier.
    #[must_use]
    pub fn container_id(&self) -> &str {
        &self.container_id
    }

    /// Return command argv entries.
    #[must_use]
    pub fn command(&self) -> &[String] {
        &self.command
    }

    /// Return configured environment variables.
    #[must_use]
    pub fn env(&self) -> Option<&[String]> {
        self.env.as_deref()
    }

    /// Return execution mode.
    #[must_use]
    pub const fn mode(&self) -> ExecMode {
        self.mode
    }

    /// Return pseudo-terminal allocation mode.
    #[must_use]
    pub const fn tty(&self) -> bool {
        self.tty
    }
}

/// Outcome of a container command execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    exec_id: String,
    exit_code: i64,
}

impl ExecResult {
    /// Return daemon-assigned exec identifier.
    #[must_use]
    pub fn exec_id(&self) -> &str {
        &self.exec_id
    }

    /// Return command exit code captured from exec inspect.
    #[must_use]
    pub const fn exit_code(&self) -> i64 {
        self.exit_code
    }
}

impl EngineConnector {
    /// Execute a command in a running container (async version).
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::ExecFailed` when command execution fails, and
    /// `ConfigError::MissingRequired` when the request is invalid.
    pub async fn exec_async<C: ContainerExecClient>(
        client: &C,
        request: &ExecRequest,
    ) -> Result<ExecResult, PodbotError> {
        Self::exec_async_with_terminal_size_provider(client, request, &SystemTerminalSizeProvider)
            .await
    }

    /// Execute a command in a running container using a caller runtime handle.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::exec_async`].
    pub fn exec<C: ContainerExecClient>(
        runtime: &tokio::runtime::Handle,
        client: &C,
        request: &ExecRequest,
    ) -> Result<ExecResult, PodbotError> {
        runtime.block_on(Self::exec_async(client, request))
    }

    async fn exec_async_with_terminal_size_provider<
        C: ContainerExecClient,
        P: TerminalSizeProvider,
    >(
        client: &C,
        request: &ExecRequest,
        size_provider: &P,
    ) -> Result<ExecResult, PodbotError> {
        let create_result = client
            .create_exec(request.container_id(), build_create_exec_options(request))
            .await
            .map_err(|error| {
                exec_failed(
                    request.container_id(),
                    format!("create exec failed: {error}"),
                )
            })?;

        let exec_id = create_result.id;
        let start_result = client
            .start_exec(&exec_id, Some(build_start_exec_options(request)))
            .await
            .map_err(|error| {
                exec_failed(
                    request.container_id(),
                    format!("start exec failed: {error}"),
                )
            })?;

        match (request.mode(), start_result) {
            (ExecMode::Attached, bollard::exec::StartExecResults::Attached { output, input }) => {
                run_attached_session_async(client, request, &exec_id, output, input, size_provider)
                    .await?;
            }
            (ExecMode::Attached, bollard::exec::StartExecResults::Detached) => {
                return Err(exec_failed(
                    request.container_id(),
                    "daemon returned detached start result for attached mode",
                ));
            }
            (ExecMode::Detached, bollard::exec::StartExecResults::Attached { .. }) => {
                return Err(exec_failed(
                    request.container_id(),
                    "daemon returned attached start result for detached mode",
                ));
            }
            (ExecMode::Detached, bollard::exec::StartExecResults::Detached) => {}
        }

        let exit_code = wait_for_exit_code_async(client, request.container_id(), &exec_id).await?;
        Ok(ExecResult { exec_id, exit_code })
    }
}

fn build_create_exec_options(request: &ExecRequest) -> CreateExecOptions<String> {
    let attached = request.mode().is_attached();
    CreateExecOptions::<String> {
        attach_stdin: Some(attached),
        attach_stdout: Some(attached),
        attach_stderr: Some(attached),
        tty: Some(attached && request.tty()),
        env: request.env().map(<[String]>::to_vec),
        cmd: Some(request.command().to_vec()),
        ..CreateExecOptions::default()
    }
}

const fn build_start_exec_options(request: &ExecRequest) -> StartExecOptions {
    StartExecOptions {
        detach: !request.mode().is_attached(),
        tty: request.mode().is_attached() && request.tty(),
        output_capacity: None,
    }
}

fn validate_command(command: Vec<String>) -> Result<Vec<String>, PodbotError> {
    if command.is_empty() {
        return Err(PodbotError::from(ConfigError::MissingRequired {
            field: String::from("command"),
        }));
    }

    let executable = command.first().map(String::as_str).unwrap_or_default();
    if executable.trim().is_empty() {
        return Err(PodbotError::from(ConfigError::InvalidValue {
            field: String::from("command"),
            reason: String::from("command executable must not be empty"),
        }));
    }

    Ok(command)
}

fn validate_required_field<'a>(field: &str, value: &'a str) -> Result<&'a str, PodbotError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(PodbotError::from(ConfigError::MissingRequired {
            field: String::from(field),
        }));
    }

    Ok(trimmed)
}

pub(super) fn exec_failed(container_id: &str, message: impl Into<String>) -> PodbotError {
    PodbotError::from(ContainerError::ExecFailed {
        container_id: String::from(container_id),
        message: message.into(),
    })
}

#[cfg(test)]
mod tests;
