//! Container command execution orchestration.
//!
//! This module provides the library-facing exec orchestration function that
//! connects to the container engine, builds an exec request, and returns
//! the command outcome. Terminal detection (whether stdin/stdout are TTYs)
//! is the caller's responsibility.

use crate::config::AppConfig;
use crate::engine::{ContainerExecClient, EngineConnector, ExecMode, ExecRequest, SocketResolver};
use crate::error::Result as PodbotResult;

use super::CommandOutcome;

/// Parameters for executing a command in a running container.
///
/// Groups the arguments required by [`exec`] into a single struct to
/// satisfy the "no more than four parameters" convention.
pub struct ExecParams<'a, E: mockable::Env> {
    /// Application configuration (provides engine socket).
    pub config: &'a AppConfig,
    /// Target container identifier or name.
    pub container: &'a str,
    /// Command argv to execute.
    pub command: Vec<String>,
    /// Attached or detached execution mode.
    pub mode: ExecMode,
    /// Whether to allocate a pseudo-terminal (only effective in attached
    /// mode). The caller is responsible for determining whether the local
    /// terminal supports TTY.
    pub tty: bool,
    /// Tokio runtime handle for blocking execution.
    pub runtime_handle: &'a tokio::runtime::Handle,
    /// Environment variable provider for socket resolution.
    pub env: &'a E,
}

/// Parameters for [`exec_with_client`] when a pre-connected engine
/// client is already available.
pub struct ExecWithClientParams<'a, C: ContainerExecClient> {
    /// Pre-connected engine client.
    pub client: &'a C,
    /// Target container identifier or name.
    pub container: &'a str,
    /// Command argv to execute.
    pub command: Vec<String>,
    /// Attached or detached execution mode.
    pub mode: ExecMode,
    /// Whether to allocate a pseudo-terminal.
    pub tty: bool,
    /// Tokio runtime handle for blocking execution.
    pub runtime_handle: &'a tokio::runtime::Handle,
}

/// Execute a command in a running container.
///
/// Resolves the engine socket, connects, builds an exec request, and
/// returns the command outcome.
///
/// # Errors
///
/// Returns `PodbotError` variants:
/// - `ContainerError::ConnectionFailed` / `SocketNotFound` /
///   `PermissionDenied` if the engine connection fails.
/// - `ContainerError::ExecFailed` if command execution fails.
/// - `ConfigError::MissingRequired` if required fields are empty.
pub fn exec<E: mockable::Env>(params: ExecParams<'_, E>) -> PodbotResult<CommandOutcome> {
    let ExecParams {
        config,
        container,
        command,
        mode,
        tty,
        runtime_handle,
        env,
    } = params;

    let resolver = SocketResolver::new(env);
    let docker =
        EngineConnector::connect_with_fallback(config.engine_socket.as_deref(), &resolver)?;

    exec_with_client(ExecWithClientParams {
        client: &docker,
        container,
        command,
        mode,
        tty,
        runtime_handle,
    })
}

/// Execute a command using a pre-connected engine client.
///
/// Builds the exec request, runs the command via the supplied client,
/// and maps the exit code to a [`CommandOutcome`]. Use this variant
/// when the caller already holds a connected client (e.g. in tests
/// with a mock [`ContainerExecClient`]).
///
/// # Errors
///
/// Returns `PodbotError` variants:
/// - `ContainerError::ExecFailed` if command execution fails.
/// - `ConfigError::MissingRequired` if required fields are empty.
pub fn exec_with_client<C: ContainerExecClient>(
    params: ExecWithClientParams<'_, C>,
) -> PodbotResult<CommandOutcome> {
    let ExecWithClientParams {
        client,
        container,
        command,
        mode,
        tty,
        runtime_handle,
    } = params;

    let request = ExecRequest::new(container, command, mode)?.with_tty(tty);
    let exec_result = EngineConnector::exec(runtime_handle, client, &request)?;

    if exec_result.exit_code() == 0 {
        Ok(CommandOutcome::Success)
    } else {
        Ok(CommandOutcome::CommandExit {
            code: exec_result.exit_code(),
        })
    }
}
