//! Git identity configuration orchestration.
//!
//! Reads Git identity from the host and configures it within the
//! container. Missing identity fields produce warnings rather than
//! errors, following the principle that Git identity is helpful but
//! not required for all container operations.

use crate::engine::{
    ContainerExecClient, GitIdentityResult, HostCommandRunner,
    configure_git_identity as engine_configure, read_host_git_identity,
};
use crate::error::Result as PodbotResult;

/// Parameters for Git identity configuration.
pub struct GitIdentityParams<'a, C: ContainerExecClient, R: HostCommandRunner> {
    /// Pre-connected container engine client.
    pub client: &'a C,
    /// Host command runner for reading Git config.
    pub host_runner: &'a R,
    /// Target container identifier.
    pub container_id: &'a str,
    /// Tokio runtime handle for blocking execution.
    pub runtime_handle: &'a tokio::runtime::Handle,
}

/// Read host Git identity and configure it in the container.
///
/// This is the top-level orchestration entry point for Step 4.1.
/// Missing host identity fields result in a partial or none-configured
/// result rather than an error.
///
/// # Errors
///
/// Returns `ContainerError::ExecFailed` if a `git config` command
/// fails to execute within the container.
pub fn configure_container_git_identity<C: ContainerExecClient, R: HostCommandRunner>(
    params: &GitIdentityParams<'_, C, R>,
) -> PodbotResult<GitIdentityResult> {
    let identity = read_host_git_identity(params.host_runner);
    engine_configure(
        params.runtime_handle,
        params.client,
        params.container_id,
        &identity,
    )
}
