//! Agent orchestration lifecycle.
//!
//! Implements the seven-step execution flow from the design document: create
//! the sandbox container, inject credentials and allowlisted environment
//! values, configure Git identity, acquire and refresh a GitHub App token,
//! prepare the workspace, and launch the agent with an attached terminal.
//!
//! Every dependency is injected through a port so the lifecycle can be tested
//! without a container engine, GitHub, or a real clock. Teardown is explicit:
//! the container is stopped and removed on success and on every failure path,
//! and the host-side token store is always cleaned up.

mod entry;
mod request;

use mockable::{Clock, Env};

use crate::api::{CommandOutcome, GitIdentityParams, RunRequest, configure_container_git_identity};
use crate::config::{AppConfig, WorkspaceSource};
use crate::engine::{
    ContainerCreator, ContainerExecClient, ContainerLifecycleClient, ContainerUploader,
    CredentialUploadRequest, EngineConnector, HostCommandRunner, RepositoryCloneRequest,
    block_on_runtime_handle, clone_repository_into_workspace,
};
use crate::error::{ConfigError, ContainerError, PodbotError, Result as PodbotResult};
use crate::github::{
    DEFAULT_EXPIRY_BUFFER, DaemonShutdown, DaemonShutdownSignal, GitHubInstallationTokenClient,
    InstallationAccessToken, RefreshContext, TokenStore, run_daemon_with_token,
};

pub use entry::run_agent;
use request::{RunPlan, agent_exec_request, build_run_plan, host_home_dir};

/// Grace period before the engine kills a container during teardown.
const TEARDOWN_GRACE_SECONDS: i32 = 10;

/// Injected dependencies for one agent run.
pub(crate) struct AgentRunner<'a, C, L, E, U, H, T>
where
    C: ContainerCreator,
    L: ContainerLifecycleClient + Sync,
    E: ContainerExecClient + Sync,
    U: ContainerUploader,
    H: HostCommandRunner,
    T: GitHubInstallationTokenClient,
{
    /// Container creation port.
    pub(crate) creator: &'a C,
    /// Container lifecycle port.
    pub(crate) lifecycle: &'a L,
    /// Container exec port.
    pub(crate) exec: &'a E,
    /// Credential upload port.
    pub(crate) uploader: &'a U,
    /// Host command runner used to read Git identity.
    pub(crate) host_runner: &'a H,
    /// GitHub token client, present only for clone-based workspaces.
    pub(crate) token_client: Option<&'a T>,
    /// Clock used for session identifiers and daemon scheduling.
    pub(crate) clock: &'a dyn Clock,
    /// Environment used for allowlisted values and the host home directory.
    pub(crate) env: &'a dyn Env,
    /// Runtime handle used for blocking engine calls.
    pub(crate) runtime: &'a tokio::runtime::Handle,
}

impl<C, L, E, U, H, T> AgentRunner<'_, C, L, E, U, H, T>
where
    C: ContainerCreator,
    L: ContainerLifecycleClient + Sync,
    E: ContainerExecClient + Sync,
    U: ContainerUploader,
    H: HostCommandRunner,
    T: GitHubInstallationTokenClient,
{
    /// Execute the full lifecycle for a run request.
    ///
    /// # Errors
    ///
    /// Returns validation errors before any engine work, and container,
    /// filesystem, GitHub, or exec errors from the lifecycle. Teardown and
    /// token-store cleanup failures are reported after the primary result.
    pub(crate) fn run(
        &self,
        config: &AppConfig,
        request: &RunRequest,
    ) -> PodbotResult<CommandOutcome> {
        let plan = build_run_plan(config, request, self.clock, self.env)?;
        let result = self.execute(config, &plan);
        let cleanup = plan
            .token_store
            .as_ref()
            .map_or(Ok(()), TokenStore::cleanup);
        combine(result, cleanup)
    }

    /// Run the container lifecycle and always attempt teardown afterwards.
    fn execute(&self, config: &AppConfig, plan: &RunPlan) -> PodbotResult<CommandOutcome> {
        tracing::info!(
            session_id = %plan.session_id,
            container_name = %plan.container_name,
            "starting agent sandbox"
        );
        let initial_token = self.prepare_token(config, plan)?;
        let container_id = self.create_and_start(plan)?;
        let work = || self.run_inside(config, plan, &container_id);
        let daemon = self.run_with_daemon(plan, initial_token, work);
        let teardown = self.teardown(&container_id);
        combine(daemon, teardown)
    }

    /// Acquire and persist the first installation token before container start.
    fn prepare_token(
        &self,
        config: &AppConfig,
        plan: &RunPlan,
    ) -> PodbotResult<Option<InstallationAccessToken>> {
        let (Some(client), Some(store)) = (self.token_client, plan.token_store.as_ref()) else {
            return Ok(None);
        };
        let installation_id = installation_id(config)?;
        let token = block_on_runtime_handle(self.runtime, async {
            client
                .acquire_installation_token(installation_id, DEFAULT_EXPIRY_BUFFER)
                .await
                .map_err(PodbotError::from)
        })?;
        store.write_token(token.token())?;
        Ok(Some(token))
    }

    /// Create and start the sandbox container.
    fn create_and_start(&self, plan: &RunPlan) -> PodbotResult<String> {
        let container_id =
            EngineConnector::create_container(self.runtime, self.creator, &plan.create_request)?;
        if let Err(error) =
            EngineConnector::start_container(self.runtime, self.lifecycle, &container_id)
        {
            // The container exists but never started; removal is best effort
            // because the start error is the failure the caller must see.
            if let Err(cleanup_error) =
                EngineConnector::remove_container(self.runtime, self.lifecycle, &container_id)
            {
                tracing::debug!(
                    %cleanup_error,
                    "failed to remove container after a start failure"
                );
            }
            return Err(error);
        }
        Ok(container_id)
    }

    /// Perform all in-container preparation and launch the agent.
    fn run_inside(
        &self,
        config: &AppConfig,
        plan: &RunPlan,
        container_id: &str,
    ) -> PodbotResult<CommandOutcome> {
        self.upload_credentials(config, container_id)?;
        self.configure_git(container_id)?;
        self.clone_workspace(config, plan, container_id)?;
        self.exec_agent(config, container_id)
    }

    /// Upload selected host credential families into the container.
    fn upload_credentials(&self, config: &AppConfig, container_id: &str) -> PodbotResult<()> {
        let home = host_home_dir(self.env)?;
        let request = CredentialUploadRequest::from_app_config(container_id, home, config);
        EngineConnector::upload_credentials(self.runtime, self.uploader, &request)?;
        Ok(())
    }

    /// Copy the host Git identity into the container.
    fn configure_git(&self, container_id: &str) -> PodbotResult<()> {
        let params = GitIdentityParams {
            client: self.exec,
            host_runner: self.host_runner,
            container_id,
            runtime_handle: self.runtime,
        };
        configure_container_git_identity(&params)?;
        Ok(())
    }

    /// Clone the repository when the workspace source requires it.
    fn clone_workspace(
        &self,
        config: &AppConfig,
        plan: &RunPlan,
        container_id: &str,
    ) -> PodbotResult<()> {
        if config.workspace.source != WorkspaceSource::GithubClone {
            return Ok(());
        }
        let request = RepositoryCloneRequest {
            container_id,
            repository: &plan.repository,
            branch: &plan.branch,
            workspace_base_dir: &plan.workspace_path,
            askpass_path: &plan.askpass_path,
        };
        clone_repository_into_workspace(self.runtime, self.exec, &request)?;
        Ok(())
    }

    /// Launch the agent and map its exit code to a command outcome.
    fn exec_agent(&self, config: &AppConfig, container_id: &str) -> PodbotResult<CommandOutcome> {
        let request = agent_exec_request(config, container_id)?;
        let engine_request = crate::engine::ExecRequest::new(
            request.container(),
            request.command().to_vec(),
            request.mode().into(),
        )?
        .with_tty(request.tty());
        let result = EngineConnector::exec(self.runtime, self.exec, &engine_request)?;
        let code = result.exit_code();
        if code == 0 {
            Ok(CommandOutcome::Success)
        } else {
            Ok(CommandOutcome::CommandExit { code })
        }
    }

    /// Run `work` while the token daemon refreshes the mounted token.
    ///
    /// The daemon runs on its own scoped thread with an independent runtime so
    /// it cannot block the attached agent session. When no token client or
    /// store is configured, `work` runs directly.
    fn run_with_daemon<F>(
        &self,
        plan: &RunPlan,
        initial_token: Option<InstallationAccessToken>,
        work: F,
    ) -> PodbotResult<CommandOutcome>
    where
        F: FnOnce() -> PodbotResult<CommandOutcome>,
    {
        let (Some(initial), Some(store), Some(client)) =
            (initial_token, plan.token_store.as_ref(), self.token_client)
        else {
            return work();
        };
        let installation_id = installation_id_from_plan(plan)?;
        let context = RefreshContext {
            client,
            installation_id,
            store,
            clock: self.clock,
        };
        let shutdown = DaemonShutdown::new();
        let signal = shutdown.subscribe();

        std::thread::scope(|scope| {
            let daemon = scope.spawn(move || daemon_loop(&context, signal, initial));
            let work_result = work();
            shutdown.shutdown();
            let daemon_result = daemon
                .join()
                .unwrap_or_else(|_| Err(daemon_thread_panicked()));
            combine(work_result, daemon_result)
        })
    }

    /// Stop and remove the container, tolerating an already-absent container.
    fn teardown(&self, container_id: &str) -> PodbotResult<()> {
        let stop_error = EngineConnector::stop_container(
            self.runtime,
            self.lifecycle,
            container_id,
            TEARDOWN_GRACE_SECONDS,
        )
        .err()
        .filter(|error| !is_not_found(error));
        let remove_error =
            EngineConnector::remove_container(self.runtime, self.lifecycle, container_id)
                .err()
                .filter(|error| !is_not_found(error));

        match (stop_error, remove_error) {
            (Some(error), _) | (None, Some(error)) => Err(error),
            (None, None) => Ok(()),
        }
    }
}

/// Run the refresh loop on the daemon thread's own runtime.
fn daemon_loop(
    context: &RefreshContext<'_>,
    mut shutdown: DaemonShutdownSignal,
    initial: InstallationAccessToken,
) -> PodbotResult<()> {
    let runtime = super::create_runtime()?;
    runtime.block_on(run_daemon_with_token(context, &mut shutdown, initial))
}

/// Resolve the installation identifier required by the daemon.
fn installation_id(config: &AppConfig) -> PodbotResult<u64> {
    config.github.installation_id.ok_or_else(|| {
        ConfigError::MissingRequired {
            field: String::from("github.installation_id"),
        }
        .into()
    })
}

/// Resolve the installation identifier from the validated run plan.
fn installation_id_from_plan(plan: &RunPlan) -> PodbotResult<u64> {
    plan.installation_id.ok_or_else(|| {
        ConfigError::MissingRequired {
            field: String::from("github.installation_id"),
        }
        .into()
    })
}

/// Return whether an error means the container no longer exists.
const fn is_not_found(error: &PodbotError) -> bool {
    matches!(
        error,
        PodbotError::Container(ContainerError::NotFound { .. })
    )
}

/// Convert a daemon-thread panic into a recoverable failure.
fn daemon_thread_panicked() -> PodbotError {
    crate::error::GitHubError::DaemonFailed {
        message: String::from("token refresh daemon thread panicked"),
    }
    .into()
}

/// Preserve the primary result while reporting a secondary failure.
fn combine(
    primary: PodbotResult<CommandOutcome>,
    secondary: PodbotResult<()>,
) -> PodbotResult<CommandOutcome> {
    match (primary, secondary) {
        (Ok(outcome), Ok(())) => Ok(outcome),
        (Ok(_), Err(error)) | (Err(error), Ok(())) => Err(error),
        (Err(error), Err(secondary_error)) => {
            tracing::warn!(
                %secondary_error,
                "secondary cleanup failed after the primary run error"
            );
            Err(error)
        }
    }
}
