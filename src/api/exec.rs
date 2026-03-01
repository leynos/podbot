//! Container command execution orchestration.
//!
//! This module provides the library-facing exec orchestration function that
//! builds an exec request, runs it via an injected
//! [`ContainerExecClient`](crate::engine::ContainerExecClient), and returns
//! the command outcome. Terminal detection (whether stdin/stdout are TTYs)
//! and engine connection are the caller's responsibility.

use crate::engine::{ContainerExecClient, EngineConnector, ExecMode, ExecRequest};
use crate::error::Result as PodbotResult;

use super::CommandOutcome;

/// Parameters for executing a command in a running container.
///
/// Groups the arguments required by [`exec`] into a single struct to
/// satisfy the "no more than four parameters" convention. The caller
/// is responsible for connecting to the container engine and supplying
/// the resulting client via the `connector` field, enabling dependency
/// injection for testability.
pub struct ExecParams<'a, C: ContainerExecClient> {
    /// Pre-connected container engine client. The CLI adapter typically
    /// obtains this via
    /// [`EngineConnector::connect_with_fallback`](crate::engine::EngineConnector::connect_with_fallback);
    /// tests supply a mock implementation.
    pub connector: &'a C,
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
}

/// Execute a command in a running container.
///
/// Builds an exec request, runs it via the supplied `connector`, and
/// maps the exit code to a [`CommandOutcome`].
///
/// # Errors
///
/// Returns `PodbotError` variants:
/// - `ContainerError::ExecFailed` if command execution fails.
/// - `ConfigError::MissingRequired` if required fields are empty.
pub fn exec<C: ContainerExecClient>(params: ExecParams<'_, C>) -> PodbotResult<CommandOutcome> {
    let ExecParams {
        connector,
        container,
        command,
        mode,
        tty,
        runtime_handle,
    } = params;

    let request = ExecRequest::new(container, command, mode)?.with_tty(tty);
    let exec_result = EngineConnector::exec(runtime_handle, connector, &request)?;

    if exec_result.exit_code() == 0 {
        Ok(CommandOutcome::Success)
    } else {
        Ok(CommandOutcome::CommandExit {
            code: exec_result.exit_code(),
        })
    }
}
