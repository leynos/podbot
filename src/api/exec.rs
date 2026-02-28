//! Container command execution orchestration.
//!
//! This module provides the library-facing exec orchestration function that
//! connects to the container engine, builds an exec request, and returns
//! the command outcome. Terminal detection (whether stdin/stdout are TTYs)
//! is the caller's responsibility.

use crate::config::AppConfig;
use crate::engine::{EngineConnector, ExecMode, ExecRequest, SocketResolver};
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

    let request = ExecRequest::new(container, command, mode)?.with_tty(tty);
    let exec_result = EngineConnector::exec(runtime_handle, &docker, &request)?;

    if exec_result.exit_code() == 0 {
        Ok(CommandOutcome::Success)
    } else {
        Ok(CommandOutcome::CommandExit {
            code: exec_result.exit_code(),
        })
    }
}
