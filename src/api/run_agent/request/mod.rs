//! Run-plan construction for agent orchestration.
//!
//! Converts the library-owned [`RunRequest`] and [`AppConfig`] into validated
//! values: container name, labels, mounts, environment allowlist, clone
//! coordinates, and the agent argv. Keeping this translation separate from the
//! lifecycle code makes each decision independently testable.

use camino::Utf8PathBuf;

use crate::api::{
    AskpassPath, BranchName, ExecMode, ExecRequest, RepositoryRef, RunRequest, WorkspacePath,
};
use crate::config::{AgentKind, AgentMode, AppConfig, WorkspaceSource};
use crate::engine::{BindMount, CreateContainerRequest, ownership_labels};
use crate::error::{ConfigError, Result as PodbotResult};
use crate::github::{CONTAINER_TOKEN_DIR, TokenStore};

use mockable::{Clock, Env};

/// In-container path of the Git askpass helper baked into the sandbox image.
pub(crate) const DEFAULT_ASKPASS_PATH: &str = "/usr/local/bin/git-askpass";

/// Validated inputs needed to create and drive one agent container.
pub(crate) struct RunPlan {
    /// Stable identifier recorded in labels and used to name the container.
    pub(crate) session_id: String,
    /// Engine container name.
    pub(crate) container_name: String,
    /// Fully populated creation request.
    pub(crate) create_request: CreateContainerRequest,
    /// Absolute in-container workspace path.
    pub(crate) workspace_path: WorkspacePath,
    /// Repository coordinates used for cloning.
    pub(crate) repository: RepositoryRef,
    /// Branch checked out in the workspace.
    pub(crate) branch: BranchName,
    /// Askpass helper path passed to Git.
    pub(crate) askpass_path: AskpassPath,
    /// Host-side token store when the workspace is cloned from GitHub.
    pub(crate) token_store: Option<TokenStore>,
    /// Installation identifier used by the refresh daemon.
    pub(crate) installation_id: Option<u64>,
}

/// Build a session identifier from the process ID and wall-clock nanoseconds.
///
/// The clock injection keeps the value deterministic in tests and avoids a
/// direct dependency on the host clock.
pub(crate) fn session_id(clock: &dyn Clock) -> String {
    let nanos = clock.utc().timestamp_nanos_opt().unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

/// Return the engine container name for a session.
pub(crate) fn container_name(session_id: &str) -> String {
    format!("podbot-{session_id}")
}

/// Resolve the in-container workspace path for the configured source.
fn workspace_path(config: &AppConfig, repository: &RepositoryRef) -> PodbotResult<WorkspacePath> {
    match config.workspace.source {
        WorkspaceSource::GithubClone => {
            let path = config.workspace.base_dir.join(repository.name());
            WorkspacePath::parse(path.as_str())
        }
        WorkspaceSource::HostMount => {
            let path = config
                .workspace
                .container_path
                .clone()
                .unwrap_or_else(crate::config::default_host_mount_container_path);
            WorkspacePath::parse(path.as_str())
        }
    }
}

/// Build bind mounts for the configured workspace source.
///
/// A directory mount is used for the token store so atomic renames remain
/// visible inside the container; a single-file bind mount would keep pointing
/// at the replaced inode after the first refresh.
fn build_mounts(
    config: &AppConfig,
    workspace_path: &WorkspacePath,
    token_store: Option<&TokenStore>,
) -> PodbotResult<Vec<BindMount>> {
    match config.workspace.source {
        WorkspaceSource::GithubClone => token_mounts(token_store),
        WorkspaceSource::HostMount => host_mounts(config, workspace_path),
    }
}

/// Mount the token directory read-only so the container can read the token
/// without being able to replace it.
fn token_mounts(token_store: Option<&TokenStore>) -> PodbotResult<Vec<BindMount>> {
    let Some(store) = token_store else {
        return Ok(Vec::new());
    };
    Ok(vec![BindMount::read_only(
        store.session_dir(),
        CONTAINER_TOKEN_DIR,
    )?])
}

/// Mount the operator-supplied host workspace read-write.
fn host_mounts(config: &AppConfig, workspace_path: &WorkspacePath) -> PodbotResult<Vec<BindMount>> {
    let host_path =
        config
            .workspace
            .host_path
            .as_deref()
            .ok_or_else(|| ConfigError::MissingRequired {
                field: String::from("workspace.host_path"),
            })?;
    Ok(vec![BindMount::read_write(
        host_path,
        workspace_path.as_str(),
    )?])
}

/// Copy allowlisted host environment variables into the container.
fn allowed_environment(config: &AppConfig, env: &dyn Env) -> Vec<String> {
    config
        .agent
        .env_allowlist
        .iter()
        .filter_map(|name| env.string(name).map(|value| format!("{name}={value}")))
        .collect()
}

/// Return whether the run requires GitHub App credentials.
pub(crate) const fn needs_github_token(config: &AppConfig) -> bool {
    matches!(config.workspace.source, WorkspaceSource::GithubClone)
}

/// Build the full plan for a run request.
///
/// # Errors
///
/// Returns validation errors for malformed repository, branch, workspace, or
/// askpass values, for a missing host workspace path, and for token-store
/// creation failures.
pub(crate) fn build_run_plan(
    config: &AppConfig,
    request: &RunRequest,
    clock: &dyn Clock,
    env: &dyn Env,
) -> PodbotResult<RunPlan> {
    let repository = RepositoryRef::parse(request.repository())?;
    let branch = BranchName::parse(request.branch())?;
    let askpass_path = AskpassPath::parse(DEFAULT_ASKPASS_PATH)?;
    let workspace_path = workspace_path(config, &repository)?;
    let session_id = session_id(clock);
    let container_name = container_name(&session_id);
    let token_store = if needs_github_token(config) {
        Some(TokenStore::from_env(env, &container_name)?)
    } else {
        None
    };

    let create_request = CreateContainerRequest::from_app_config(config)?
        .with_name(Some(container_name.clone()))
        .with_labels(ownership_labels(
            &session_id,
            request.repository(),
            request.branch(),
        ))
        .with_mounts(build_mounts(config, &workspace_path, token_store.as_ref())?)
        .with_working_dir(Some(String::from(workspace_path.as_str())))
        .with_env(Some(allowed_environment(config, env)));

    Ok(RunPlan {
        session_id,
        container_name,
        create_request,
        workspace_path,
        repository,
        branch,
        askpass_path,
        token_store,
        installation_id: config.github.installation_id,
    })
}

/// Build the agent argv for the interactive execution path.
///
/// # Errors
///
/// Returns `ConfigError::InvalidValue` for a non-interactive agent mode and
/// `ConfigError::MissingRequired` when a custom agent has no executable.
pub(crate) fn agent_command(config: &AppConfig) -> PodbotResult<Vec<String>> {
    if config.agent.mode != AgentMode::Podbot {
        return Err(ConfigError::InvalidValue {
            field: String::from("agent.mode"),
            reason: String::from("podbot run supports only agent.mode = \"podbot\""),
        }
        .into());
    }

    let mut command = match config.agent.kind {
        AgentKind::Claude => vec![
            String::from("claude"),
            String::from("--dangerously-skip-permissions"),
        ],
        AgentKind::Codex => vec![
            String::from("codex"),
            String::from("--dangerously-bypass-approvals-and-sandbox"),
        ],
        AgentKind::Custom => vec![custom_executable(config)?],
    };
    command.extend(config.agent.args.iter().cloned());
    Ok(command)
}

/// Return the validated custom agent executable.
fn custom_executable(config: &AppConfig) -> PodbotResult<String> {
    config
        .agent
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
        .ok_or_else(|| {
            ConfigError::MissingRequired {
                field: String::from("agent.command"),
            }
            .into()
        })
}

/// Build the attached exec request that launches the agent.
///
/// # Errors
///
/// Returns validation errors from [`ExecRequest::new`] or from
/// [`agent_command`].
pub(crate) fn agent_exec_request(
    config: &AppConfig,
    container_id: &str,
) -> PodbotResult<ExecRequest> {
    Ok(ExecRequest::new(container_id, agent_command(config)?)?
        .with_mode(ExecMode::Attached)
        .with_tty(true))
}

/// Format the host home directory for credential upload.
///
/// # Errors
///
/// Returns `ConfigError::MissingRequired` when `HOME` is unset or blank.
pub(crate) fn host_home_dir(env: &dyn Env) -> PodbotResult<Utf8PathBuf> {
    env.string("HOME")
        .filter(|value| !value.trim().is_empty())
        .map(Utf8PathBuf::from)
        .ok_or_else(|| {
            ConfigError::MissingRequired {
                field: String::from("HOME"),
            }
            .into()
        })
}

#[cfg(test)]
mod tests;
